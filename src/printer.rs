// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Native printer submission on a dedicated thread.
//!
//! A successful OS spool call is only an *acceptance*, not proof that paper was
//! printed. The durable print ledger remains pending until human verification
//! or a future driver-specific completion signal.
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;
use std::{
    path::{Path, PathBuf},
    sync::{mpsc::{self, Receiver, SyncSender, TrySendError}, Arc},
    thread,
};

pub struct PrintRequest {
    pub job_id: String,
    pub printer: String,
    pub jpeg: PathBuf,
}

pub enum PrintEvent {
    Submitted { job_id: String, spooler_id: String },
    Failed { job_id: String, reason: String },
}

pub struct PrinterWorker {
    tx: SyncSender<PrintRequest>,
    pub events: Receiver<PrintEvent>,
}

impl PrinterWorker {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, requests) = mpsc::sync_channel::<PrintRequest>(2);
        let (reply, events) = mpsc::channel();
        thread::Builder::new().name("bs-printer".into()).spawn(move || {
            while let Ok(request) = requests.recv() {
                let job_id = request.job_id.clone();
                let result = submit(&request.printer, &request.jpeg).map(|spooler_id| {
                    PrintEvent::Submitted { job_id: job_id.clone(), spooler_id }
                }).unwrap_or_else(|error| PrintEvent::Failed {
                    job_id, reason: error.to_string(),
                });
                if reply.send(result).is_err() { break; }
                wake();
            }
        }).expect("spawn worker thread");
        Self { tx, events }
    }

    pub fn queue(&self, request: PrintRequest) -> Result<(), &'static str> {
        self.tx.try_send(request).map_err(|error| match error {
            TrySendError::Full(_) => "Printer queue is full",
            TrySendError::Disconnected(_) => "Printer worker is unavailable",
        })
    }
}

/// Discover installed OS print queues, not physical USB connections.
/// Calling this on demand avoids background polling of drivers/spoolers.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn list_printers() -> std::io::Result<Vec<String>> {
    use std::io;
    let output = Command::new("lpstat").arg("-p").env("LC_ALL", "C").output()?;
    if !output.status.success() {
        return Err(io::Error::other("CUPS printer enumeration failed"));
    }
    let mut printers: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("printer "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| valid_printer_name(name))
        .take(128)
        .map(ToOwned::to_owned)
        .collect();
    printers.sort_unstable();
    printers.dedup();
    Ok(printers)
}

#[cfg(target_os = "windows")]
pub fn list_printers() -> std::io::Result<Vec<String>> {
    use std::{io, mem, ptr, slice};
    use windows_sys::Win32::Graphics::Printing::{
        EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
    };
    const LEVEL: u32 = 4;
    const MAX_BUFFER: usize = 8 * 1024 * 1024;
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut needed = 0;
    let mut count = 0;
    // SAFETY: Windows writes required size and count to valid stack pointers.
    unsafe { EnumPrintersW(flags, ptr::null(), LEVEL, ptr::null_mut(), 0, &mut needed, &mut count); }
    if needed == 0 { return Ok(Vec::new()); }
    if needed as usize > MAX_BUFFER {
        return Err(io::Error::other("Printer enumeration buffer exceeds limit"));
    }
    // Use usize storage to guarantee alignment for PRINTER_INFO_4W.
    let word_size = mem::size_of::<usize>();
    let mut words = vec![0usize; (needed as usize).div_ceil(word_size)];
    let capacity = (words.len() * word_size) as u32;
    let data = words.as_mut_ptr().cast::<u8>();
    // SAFETY: `words` remains alive and aligned until all names are copied.
    // EnumPrintersW writes at most `capacity` bytes into this owned allocation.
    let success = unsafe {
        EnumPrintersW(flags, ptr::null(), LEVEL, data, capacity, &mut needed, &mut count)
    };
    if success == 0 { return Err(io::Error::last_os_error()); }
    if count as usize > capacity as usize / mem::size_of::<PRINTER_INFO_4W>() {
        return Err(io::Error::other("Invalid printer count"));
    }
    // SAFETY: The buffer is aligned and each item is filled by the Windows API.
    let records = unsafe { slice::from_raw_parts(data.cast::<PRINTER_INFO_4W>(), count as usize) };
    let mut printers = Vec::new();
    for record in records.iter().take(128) {
        let ptr = record.pPrinterName;
        if ptr.is_null() { continue; }
        // SAFETY: Windows returns a null-terminated name. Bound scanning to
        // 512 UTF-16 units to avoid unbounded allocations from corrupt data.
        let mut len = 0usize;
        unsafe { while len < 512 && *ptr.add(len) != 0 { len += 1; } }
        if len == 512 { continue; }
        // SAFETY: Units [0,len) were accessed during the bounded scan above.
        let name = String::from_utf16_lossy(unsafe { slice::from_raw_parts(ptr, len) });
        if valid_printer_name(&name) { printers.push(name); }
    }
    printers.sort_unstable();
    printers.dedup();
    Ok(printers)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn list_printers() -> std::io::Result<Vec<String>> { Ok(Vec::new()) }

/// The operating system's default print queue, used when no printer was chosen
/// in maintenance: printing works out of the box on a configured machine.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn default_printer() -> Option<String> {
    let output = Command::new("lpstat").arg("-d").env("LC_ALL", "C").output().ok()?;
    if !output.status.success() { return None; }
    String::from_utf8_lossy(&output.stdout).lines()
        .find_map(|line| line.strip_prefix("system default destination:"))
        .map(|name| name.trim().to_owned())
        .filter(|name| valid_printer_name(name))
}

#[cfg(target_os = "windows")]
pub fn default_printer() -> Option<String> {
    use windows_sys::Win32::Graphics::Printing::GetDefaultPrinterW;
    let mut buffer = [0u16; 512];
    let mut len = buffer.len() as u32;
    // SAFETY: `buffer` holds `len` UTF-16 units; Windows writes a NUL-terminated
    // name and updates `len` (including the terminator) on success.
    let ok = unsafe { GetDefaultPrinterW(buffer.as_mut_ptr(), &mut len) };
    if ok == 0 || len == 0 || len as usize > buffer.len() { return None; }
    let name = String::from_utf16_lossy(&buffer[..len as usize - 1]);
    valid_printer_name(&name).then_some(name)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn default_printer() -> Option<String> { None }

/// Where the print goes on the page (x, y, width, height in device pixels):
/// the whole page when the proportions match within 2 % (borderless prints),
/// otherwise as large as possible, centred, never distorted.
#[cfg_attr(not(windows), allow(dead_code))]
fn fit_on_page(width: u32, height: u32, page_width: i32, page_height: i32) -> (i32, i32, i32, i32) {
    if width == 0 || height == 0 || page_width <= 0 || page_height <= 0 { return (0, 0, page_width, page_height); }
    let image = f64::from(width) / f64::from(height);
    let page = f64::from(page_width) / f64::from(page_height);
    if (image / page - 1.0).abs() <= 0.02 { return (0, 0, page_width, page_height); }
    let scale = (f64::from(page_width) / f64::from(width)).min(f64::from(page_height) / f64::from(height));
    let (w, h) = ((f64::from(width) * scale).round() as i32, (f64::from(height) * scale).round() as i32);
    ((page_width - w) / 2, (page_height - h) / 2, w, h)
}

/// Printer names are always passed as one argument, never through a shell.
/// The configured printer is an administrator-controlled setting.
pub fn valid_printer_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn submit(printer: &str, jpeg: &Path) -> std::io::Result<String> {
    use std::io;
    if !valid_printer_name(printer) || jpeg.extension().is_none_or(|ext| ext != "jpg") || !jpeg.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid printer or JPEG"));
    }
    // CUPS: one immutable JPEG, one copy, a chosen administrator-controlled queue.
    // fit-to-page keeps the proportions; CUPS turns a portrait print onto
    // landscape paper by itself (orientation left to auto).
    let result = Command::new("lp")
        .args(["-d", printer, "-n", "1", "-o", "fit-to-page", "--"])
        .arg(jpeg)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!("CUPS rejected job: {}",
            String::from_utf8_lossy(&result.stderr).chars().take(240).collect::<String>())));
    }
    let message = String::from_utf8_lossy(&result.stdout);
    Ok(message.chars().take(120).collect())
}

#[cfg(target_os = "windows")]
fn submit(printer: &str, jpeg: &Path) -> std::io::Result<String> {
    windows_gdi::submit(printer, jpeg)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn submit(_printer: &str, _jpeg: &Path) -> std::io::Result<String> {
    Err(std::io::Error::other("No printing backend for this platform"))
}

#[cfg(target_os = "windows")]
mod windows_gdi {
    use super::valid_printer_name;
    use crate::image_io;
    use std::{io, iter, mem, path::Path, ptr};
    use windows_sys::Win32::Graphics::Gdi::{
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateDCW, DeleteDC,
        DIB_RGB_COLORS, GetDeviceCaps, HORZRES, SRCCOPY, StretchDIBits, VERTRES,
    };
    // Print jobs (StartDoc…) live in the XPS print module of windows-sys.
    use windows_sys::Win32::Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};

    /// Convert the owned RGB JPEG buffer into a 24-bit DIB in place.
    /// A row may need up to three bytes of padding, but no second full-size
    /// RGB/BGR allocation is needed. Reverse row copying prevents overlap.
    fn into_padded_bgr(mut rgb: Vec<u8>, width: usize, height: usize) -> io::Result<(Vec<u8>, usize)> {
        let packed_stride = width.checked_mul(3).ok_or_else(|| io::Error::other("Invalid RGB width"))?;
        let row_stride = packed_stride.checked_add(3).ok_or_else(|| io::Error::other("Invalid RGB row"))? & !3;
        let expected = packed_stride.checked_mul(height).ok_or_else(|| io::Error::other("Invalid RGB size"))?;
        let padded = row_stride.checked_mul(height).ok_or_else(|| io::Error::other("Invalid DIB size"))?;
        if rgb.len() != expected { return Err(io::Error::other("RGB length mismatch")); }
        if row_stride != packed_stride {
            rgb.resize(padded, 0);
            for row in (0..height).rev() {
                let from = row * packed_stride;
                rgb.copy_within(from..from + packed_stride, row * row_stride);
            }
        }
        for row in rgb.chunks_exact_mut(row_stride) {
            for pixel in row[..packed_stride].chunks_exact_mut(3) {
                pixel.swap(0, 2);
            }
            row[packed_stride..].fill(0);
        }
        Ok((rgb, row_stride))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn rgb_to_bgr_handles_padded_and_unpadded_rows() {
            let (rgb, stride) = into_padded_bgr(vec![1, 2, 3, 4, 5, 6], 1, 2).unwrap();
            assert_eq!(stride, 4);
            assert_eq!(rgb, vec![3, 2, 1, 0, 6, 5, 4, 0]);
            let (rgb, stride) = into_padded_bgr(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12], 4, 1).unwrap();
            assert_eq!(stride, 12);
            assert_eq!(rgb, vec![3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
        }
    }

    /// Windows GDI prints a decoded top-down 24-bit DIB through the selected
    /// vendor driver; no ShellExecute, scripts or arbitrary commands.
    pub fn submit(printer: &str, jpeg: &Path) -> io::Result<String> {
        if !valid_printer_name(printer) || !jpeg.is_file() || jpeg.extension().is_none_or(|e| e != "jpg") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid printer or JPEG"));
        }
        // Page orientation is only known once the printer DC exists: rotated below if needed.
        let rgb = image_io::read_print_jpeg(jpeg)?;
        let driver: Vec<u16> = "WINSPOOL".encode_utf16().chain(iter::once(0)).collect();
        let name: Vec<u16> = printer.encode_utf16().chain(iter::once(0)).collect();
        let title: Vec<u16> = "BoothSoft Photo".encode_utf16().chain(iter::once(0)).collect();
        // SAFETY: All pointers refer to null-terminated strings or valid buffers
        // owned for the duration of the synchronous GDI submission. Only GDI
        // handles returned by CreateDCW are used, then released with DeleteDC.
        unsafe {
            let hdc = CreateDCW(driver.as_ptr(), name.as_ptr(), ptr::null(), ptr::null());
            if hdc.is_null() { return Err(io::Error::last_os_error()); }
            let doc = DOCINFOW {
                cbSize: mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: title.as_ptr(),
                lpszOutput: ptr::null(),
                lpszDatatype: ptr::null(),
                fwType: 0,
            };
            let job = StartDocW(hdc, &doc);
            if job <= 0 {
                DeleteDC(hdc);
                return Err(io::Error::last_os_error());
            }
            let page_width = GetDeviceCaps(hdc, HORZRES as i32);
            let page_height = GetDeviceCaps(hdc, VERTRES as i32);
            // A portrait print on landscape paper (DNP QW410: 15×10) is turned a
            // quarter turn; never stretched out of proportion.
            let mut image = rgb;
            if (image.height() > image.width()) != (page_height > page_width) && page_width > 0 && page_height > 0 {
                image = image::imageops::rotate90(&image);
            }
            let (width, height) = image.dimensions();
            let (dest_x, dest_y, dest_w, dest_h) = super::fit_on_page(width, height, page_width, page_height);
            let (bgr, _) = match into_padded_bgr(image.into_raw(), width as usize, height as usize) {
                Ok(v) => v,
                Err(error) => { AbortDoc(hdc); DeleteDC(hdc); return Err(error); }
            };
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // top-down rows
                    biPlanes: 1,
                    biBitCount: 24,
                    biCompression: BI_RGB,
                    biSizeImage: bgr.len() as u32,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [mem::zeroed()],
            };
            let ok = page_width > 0 && page_height > 0 && StartPage(hdc) > 0
                && StretchDIBits(hdc, dest_x, dest_y, dest_w, dest_h,
                    0, 0, width as i32, height as i32,
                    bgr.as_ptr().cast(), &info, DIB_RGB_COLORS, SRCCOPY) > 0
                && EndPage(hdc) > 0 && EndDoc(hdc) > 0;
            if !ok { AbortDoc(hdc); }
            DeleteDC(hdc);
            if ok { Ok(job.to_string()) } else { Err(io::Error::other("GDI print submission failed")) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prints_keep_their_proportions() {
        // DNP 15×10 page, landscape print: full page.
        assert_eq!(fit_on_page(1800, 1200, 1844, 1240), (0, 0, 1844, 1240));
        // Square image on a 3:2 page: centred, not stretched.
        assert_eq!(fit_on_page(1000, 1000, 1800, 1200), (300, 0, 1200, 1200));
    }

    #[test]
    fn printer_names_reject_controls() {
        assert!(!valid_printer_name(""));
        assert!(!valid_printer_name("Printer\nInjected"));
        assert!(valid_printer_name("DNP DS620"));
    }
}
