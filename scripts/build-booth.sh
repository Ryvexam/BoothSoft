#!/usr/bin/env sh
# SPDX-License-Identifier: CPAL-1.0
# Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
# Release build tuned for the booth PC (Intel Core i5 4th/5th gen: Haswell/Broadwell,
# AVX2 + FMA, Intel HD 4600/5500 iGPU). Do NOT ship this binary to older CPUs.
# SIMD libraries (fast_image_resize, libjpeg-turbo) already pick AVX2 at runtime;
# this flag additionally vectorises BoothSoft's own loops (filters, motion gate).
set -e
RUSTFLAGS="${RUSTFLAGS} -C target-cpu=haswell" cargo build --release "$@"
