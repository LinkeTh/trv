use super::*;

impl App {
    // ── Push to device ────────────────────────────────────────────────────────

    pub(super) fn rotate_next_manual_orientation(&mut self) {
        let (code, next_idx) = next_rotation_code(self.next_rotation_code_idx);
        self.next_rotation_code_idx = next_idx;
        self.start_rotation_worker(RotationAction::RawCode(code));
    }

    pub(super) fn enable_auto_rotation(&mut self) {
        self.start_rotation_worker(RotationAction::EnableAuto);
    }

    fn start_rotation_worker(&mut self, action: RotationAction) {
        if self.rotate_result_rx.is_some() {
            self.push_status = PushStatus::Err("rotation already in progress".into());
            self.log_event("Rotation skipped: operation already in progress");
            return;
        }

        if self.push_result_rx.is_some() {
            self.push_status = PushStatus::Err("push in progress; wait before rotating".into());
            self.log_event("Rotation skipped: push still in progress");
            return;
        }

        let host = self.host.clone();
        let port = self.port;
        let recv_timeout_ms = self.recv_timeout_ms;

        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        self.rotate_result_rx = Some(rx);
        self.push_status = PushStatus::RotateInProgress;
        match action {
            RotationAction::RawCode(code) => {
                self.log_event(format!("Rotation started: code {:02X}", code.as_u8()));
            }
            RotationAction::EnableAuto => {
                self.log_event("Rotation started: enable auto-rotation");
            }
        }

        let handle = std::thread::spawn(move || {
            let result = match action {
                RotationAction::RawCode(code) => {
                    let frame = match crate::protocol::cmd::build_cmd38_frame(code) {
                        Ok(f) => f,
                        Err(e) => {
                            let _ = tx.send(Err(format!("build cmd38 frame: {}", e)));
                            return;
                        }
                    };

                    let rt = match tokio::runtime::Builder::new_current_thread()
                        .enable_io()
                        .enable_time()
                        .build()
                    {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = tx.send(Err(format!("create runtime: {}", e)));
                            return;
                        }
                    };

                    rt.block_on(async move {
                        crate::device::connection::send_frame(&host, port, &frame, recv_timeout_ms)
                            .await
                            .map_err(|e| format!("sending cmd38: {}", e))?;
                        Ok(format!("rotation code {:02X} applied", code.as_u8()))
                    })
                }
                RotationAction::EnableAuto => {
                    if !crate::device::adb::adb_available() {
                        Err("adb not found in PATH (required for auto-rotation)".into())
                    } else if !crate::device::adb::adb_settings_put_system(
                        "accelerometer_rotation",
                        "1",
                    ) {
                        Err("failed to enable auto-rotation via adb".into())
                    } else {
                        Ok("auto-rotation enabled".to_string())
                    }
                }
            };

            let _ = tx.send(result);
        });

        self.rotate_worker = Some(handle);
    }

    /// Start a background push of the current theme to the device.
    ///
    /// The worker thread pushes local assets first, then sends cmd3A frames.
    /// Completion status is returned via `push_result_rx` and polled by the
    /// event loop.
    pub(super) fn push_to_device(&mut self) {
        if self.push_result_rx.is_some() {
            self.push_status = PushStatus::Err("push already in progress".into());
            self.log_event("Push skipped: operation already in progress");
            return;
        }

        let theme = match &self.theme {
            Some(t) => t.clone(),
            None => {
                self.push_status = PushStatus::Err("no theme loaded".into());
                self.log_event("Push failed: no theme loaded");
                return;
            }
        };

        let frames = match crate::daemon::runner::build_theme_frames(&theme) {
            Ok(f) => f,
            Err(e) => {
                self.push_status = PushStatus::Err(format!("build frames: {}", e));
                self.log_event(format!("Push failed while building frames: {}", e));
                return;
            }
        };

        let host = self.host.clone();
        let port = self.port;
        let recv_timeout_ms = self.recv_timeout_ms;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_worker = Arc::clone(&cancel);
        let inter_frame_delay_ms = crate::device::connection::INTER_FRAME_DELAY.as_millis() as u64;

        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        self.push_result_rx = Some(rx);
        self.push_cancel = Some(cancel);
        self.push_status = PushStatus::PushInProgress;
        self.log_event(format!("Push started ({} frames)", frames.len()));

        let handle = std::thread::spawn(move || {
            crate::daemon::runner::push_theme_assets(&theme, false, Some(&cancel_worker));

            if cancel_worker.load(Ordering::Relaxed) {
                let _ = tx.send(Err("push cancelled".into()));
                return;
            }

            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(Err(format!("create runtime: {}", e)));
                    return;
                }
            };

            let result = rt.block_on(async move {
                for (i, frame) in frames.iter().enumerate() {
                    if cancel_worker.load(Ordering::Relaxed) {
                        return Err("push cancelled".to_string());
                    }

                    crate::device::connection::send_frame(&host, port, frame, recv_timeout_ms)
                        .await
                        .map_err(|e| format!("sending frame {}: {}", i, e))?;

                    if inter_frame_delay_ms > 0 && i + 1 < frames.len() {
                        tokio::time::sleep(Duration::from_millis(inter_frame_delay_ms)).await;
                    }
                }
                Ok(())
            });

            let _ = tx.send(result);
        });

        self.push_worker = Some(handle);
    }
}
