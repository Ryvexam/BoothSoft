#!/bin/bash
# SPDX-License-Identifier: CPAL-1.0
# Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
# Double-clic : compile si besoin puis lance BoothSoft sous son gardien (relance automatique).
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release && exec target/release/boothsoft-guard
