# Performance engineering and measurement

BoothSoft prioritizes **low idle CPU, bounded RAM, modest GPU uploads, and predictable capture latency**. A small binary is not the same as a low-memory or low-CPU program.

No actual process CPU/RAM/GPU measurements were available when this source preview was prepared. The examples below are measurement recipes, **not published benchmark results**.

## Changes in 0.9 and 0.10

| Hot path | Before | After | Rationale |
| --- | --- | --- | --- |
| Idle egui update | Timer wake every 100 ms in Ready | Wake only on camera events or user input | Avoid pointless UI passes |
| Camera preview backlog | Two RGB frames plus statuses in channel | One overwriteable RGB frame, separate capture/status queue | Bounded retained preview memory |
| Preview with inactive guest screen | Continue decoding during Review | Pause preview decoding until return/retake | Reduce CPU/GPU work |
| Filters | New image built using per-pixel floats | Operate on owned buffer using integer arithmetic | Avoid a second RGB buffer and FP conversions |
| Countdown | About 30 updates/s | About 1 text update/s | Only visible digits change every second |
| Release profile | `opt-level = "z"` | `opt-level = 2` | Throughput-oriented default; compare with size profile |

In 0.10 the costly original/framed JPEG encoding runs on a **single-flight photo worker** with a `sync_channel(1)`. The printer also has a bounded worker queue of two requests. This bounds queued work but does **not** eliminate the transient decoded capture, frame composition or graphics-driver allocations. Always measure *peak RSS* during 12–24 megapixel captures.

The camera requests a 1280×720 / 15 FPS MJPEG-or-YUYV stream where supported. In a fallback path, the camera driver may select a larger native stream; the preview is still downscaled to 854×480 and UI uploads are throttled by `PREVIEW_INTERVAL`.

The preview RGB slot holds at most `854 * 480 * 3 = 1,229,760` bytes of pixel data (excluding allocator metadata); the egui `ColorImage` and GPU texture are separate copies, and driver decode memory may be much larger. The requested preview is capped at ten frames per second in code, but actual delivery depends on the camera driver and the UI scheduler.

## Headless, reproducible CPU microbenchmark

```bash
cargo run --release --bin boothsoft-bench -- 30
cargo run --profile release-small --bin boothsoft-bench -- 30
```

Benchmark inputs are deterministic: preview filter at 854×480, composition from 1920×1080 into a 1600×1200 frame. The executable reports wall time per operation and intentionally does **not** report camera FPS, process RSS or GPU utilization. Compare on the **same powered, idle machine**, with similar temperature and driver versions. Keep the unfiltered original input identical across runs.

## Application-level process measurements

**Windows PowerShell** (run in a separate terminal with BoothSoft open):

```powershell
Get-Process boothsoft | Select-Object Id, CPU, WorkingSet64, PrivateMemorySize64
```

`CPU` is cumulative processor seconds, not an instantaneous percent. Record differences over a fixed interval; working-set bytes also include resident native libraries and graphics-driver allocations.

**Linux**:

```bash
ps -C boothsoft -o pid,%cpu,rss,vsz,etime,cmd
# When installed: pidstat -p <PID> -u -r 1 60
```

**macOS**:

```bash
ps -A -o pid,%cpu,rss,comm | grep boothsoft
# Activity Monitor: CPU, Memory, Energy and GPU usage where supported
```

GPU measurements are platform-dependent; Windows Task Manager and platform vendor profilers are more reliable than invented estimates. A framebuffer upload is not equivalent to total GPU memory use.

## Test scenarios

1. Start BoothSoft in demo mode; measure startup, idle CPU/RSS, theme changes and countdown.
2. Run with the real USB camera for at least 30 minutes; measure Preview vs Review/paused, camera reconnects and dropped frames.
3. Capture 100 photos with each filter; record peak RSS during off-thread encoding and confirm no 500 ms GUI freeze.
4. Change themes repeatedly (including large but valid PNGs), inspect retained memory and texture counts.
5. Test low-disk conditions, interrupted writes, printer disconnects, disabled quotas, ambiguous spooler results and offline boot.
6. Repeat on Windows, macOS and Linux, record device, OS, GPU/driver and Rust version.

## Performance change policy

Do not introduce: periodic network polling in the render loop, per-frame filesystem scans, unbounded queues, GPU texture creation each frame, or always-on full-resolution photo processing.

When submitting a performance PR, include a short benchmark table with **before/after median**, environment, measurement method, input sizes and source revision. Optimize wall time and memory together; never regress image quality solely to win a synthetic benchmark without discussion.

### Direct RGB alpha compositing (v0.10.0)

The print compositor blends PNG RGBA pixels directly into its owned RGB
output rather than materializing a second full-size RGBA image and another
RGB conversion. This removes two image-sized temporary allocations from the
composition stage. Pixel-format correctness is covered by opaque, transparent,
and partial-alpha unit tests. This is a code-level allocation improvement;
**peak RSS savings have not been measured**.


## v0.13 reference optimization experiments

1. Measure image composition peak RSS with a 3200x3200 photo and matching 3200x3200 frame. Owned composition should avoid an extra full-size RGB clone (about 29.3 MiB of payload for that resolution); actual allocator savings must be measured.
2. Repeat capture/export with a theme that has no active frame. The worker should encode the original once and copy the encoded JPEG instead of encoding a second time. Test byte equality and wall time, not only RSS.
3. Compare CPU idle / live preview / countdown / review on the same camera, format, GPU backend and OS settings; the camera driver may continue using resources independently of the app.
4. Validate that quota edit gestures never perform filesystem writes until Save; verify bonus unlock persists exactly once.

**No performance claims are proven until measured on target hardware.**

## v0.14 housekeeping / allocation notes

- The interactive executable, diagnostics, and benchmarks import one shared library rather than duplicate module sources and suppress dead-code warnings. This improves maintainability and is not itself evidence of lower runtime RSS.
- Review textures now borrow small RGB images rather than cloning them unconditionally before conversion to egui `ColorImage`. A thumbnail still allocates when the source is larger than 1200 pixels.
- Contact-sheet export loads, resizes and releases each input photo in turn, instead of collecting every full-resolution image in a vector. The output canvas remains 1200x1800.
- A camera still capture pushes the next preview deadline forward, avoiding an immediate redundant second decode.
- While awaiting a still capture, the UI wakes on the camera event or the eight-second deadline, instead of polling every 200ms.
- These are code-path changes only. Benchmark before attributing numerical RAM, CPU or GPU improvements.

## v0.15: bounded photo resizing and texture lifetime

- The print compositor now crops the original image in source coordinates and resizes only that *view*. A 16x4096 portrait source cropped into a 600x600 print previously required a scaled full source near 600x153600 pixels (**~264 MiB of raw RGB payload**), before final crop. The new algorithm needs the input and the final 600x600 RGB output instead; internal resampling allocations and actual peak RSS remain implementation-dependent.
- Still JPEG encoding finishes and closes the file before atomic rename. This is a correctness fix, not a throughput claim.
- After a photo is saved, the live preview texture is dropped while showing the static composed photo, and likewise on entry to maintenance. This avoids retaining an unused GPU allocation; driver-level resource release is asynchronous and unmeasured.
- The microbenchmark includes a 320x4096 -> 1600x1200 cover-layout case. Run on the same hardware and compare before/after revisions; record wall time, peak RSS and image quality. No measured performance numbers are asserted here.

## v0.16: bounded media decode and Windows DIB ownership

JPEG print imports are rejected before full decode if metadata exceeds 8,192 pixels per edge or 24,000,000 total pixels. Image-reader allocation limits add another best-effort bound. PNG frame decoding enforces a 3,200-pixel edge limit before allocation. These are safety caps, **not** measured resident-set-size results.

On Windows, the GDI adapter uses `RgbImage::into_raw()`, pads rows in reverse order when needed, then swaps RGB/BGR channels in place. Compared with the prior separate RGB + BGR buffers, this removes one retained image-sized buffer. The decoded JPEG, GPU textures, spooler, driver and OS may still hold additional copies. Record `Private Bytes`, working set and GPU committed memory on the target machine with its intended camera, paper size and DNP driver.

Persistent JSON writes now sync the complete temporary file before replacement. This may add disk I/O on quota transitions; those writes are deliberately outside the hot camera/render loop and must not be optimized away at the cost of print accounting safety.
