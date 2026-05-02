//! シングルインスタンス制御（Windows Named Mutex + Named Pipe IPC）
//!
//! 起動時に既存インスタンスの有無を Named Mutex で検出し、
//! 既存があれば Named Pipe 経由でファイルパスを送信して終了する。

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use eframe::egui;

// --- Win32 API 定数 ---
const ERROR_ALREADY_EXISTS: u32 = 183;
const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;
const GENERIC_WRITE: u32 = 0x4000_0000;
const OPEN_EXISTING: u32 = 3;
const PIPE_ACCESS_INBOUND: u32 = 0x0000_0001;
const PIPE_TYPE_MESSAGE: u32 = 0x0000_0004;
const PIPE_READMODE_MESSAGE: u32 = 0x0000_0002;
const PIPE_WAIT: u32 = 0x0000_0000;
const ERROR_MORE_DATA: u32 = 234;

const MUTEX_NAME: &str = "Local\\popone_viewer_single_instance";
const PIPE_NAME: &str = "\\\\.\\pipe\\popone_viewer_ipc";

extern "system" {
    fn CreateMutexW(
        lp_mutex_attributes: *mut std::ffi::c_void,
        b_initial_owner: i32,
        lp_name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;

    fn CreateNamedPipeW(
        lp_name: *const u16,
        dw_open_mode: u32,
        dw_pipe_mode: u32,
        n_max_instances: u32,
        n_out_buffer_size: u32,
        n_in_buffer_size: u32,
        n_default_time_out: u32,
        lp_security_attributes: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn ConnectNamedPipe(
        h_named_pipe: *mut std::ffi::c_void,
        lp_overlapped: *mut std::ffi::c_void,
    ) -> i32;
    fn DisconnectNamedPipe(h_named_pipe: *mut std::ffi::c_void) -> i32;

    fn WaitNamedPipeW(lp_named_pipe_name: *const u16, n_time_out: u32) -> i32;
    fn CreateFileW(
        lp_file_name: *const u16,
        dw_desired_access: u32,
        dw_share_mode: u32,
        lp_security_attributes: *mut std::ffi::c_void,
        dw_creation_disposition: u32,
        dw_flags_and_attributes: u32,
        h_template_file: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn WriteFile(
        h_file: *mut std::ffi::c_void,
        lp_buffer: *const u8,
        n_number_of_bytes_to_write: u32,
        lp_number_of_bytes_written: *mut u32,
        lp_overlapped: *mut std::ffi::c_void,
    ) -> i32;
    fn ReadFile(
        h_file: *mut std::ffi::c_void,
        lp_buffer: *mut u8,
        n_number_of_bytes_to_read: u32,
        lp_number_of_bytes_read: *mut u32,
        lp_overlapped: *mut std::ffi::c_void,
    ) -> i32;
    fn SetNamedPipeHandleState(
        h_named_pipe: *mut std::ffi::c_void,
        lp_mode: *const u32,
        lp_max_collection_count: *mut u32,
        lp_collect_data_timeout: *mut u32,
    ) -> i32;
}

/// Win32 ハンドルの RAII ラッパー（Drop で自動 CloseHandle）
struct WinHandle(*mut std::ffi::c_void);

impl WinHandle {
    fn new(h: *mut std::ffi::c_void) -> Option<Self> {
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(Self(h))
        }
    }
    fn as_raw(&self) -> *mut std::ffi::c_void {
        self.0
    }
}

impl Drop for WinHandle {
    fn drop(&mut self) {
        // SAFETY: self.0 is a valid handle (checked in new()).
        unsafe { CloseHandle(self.0) };
    }
}

/// &str → null 終端 UTF-16
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// シングルインスタンス判定結果
pub enum InstanceCheck {
    /// 最初のインスタンス（ログローテーション可）
    Primary,
    /// 既存インスタンスへ転送成功（即座に終了してよい）
    Forwarded,
    /// 既存インスタンスを検出したが IPC 失敗（ログローテーション不可）
    FallbackStart,
}

/// 既存インスタンスの有無を判定し、あればファイルパスを送信する。
pub fn try_send_to_existing(file_path: Option<&Path>) -> InstanceCheck {
    let mutex_name = to_wide(MUTEX_NAME);

    // SAFETY: All Win32 API calls receive valid pointers — mutex_name and pipe_name
    // are null-terminated UTF-16 from to_wide(), payload is a valid byte slice,
    // and all returned handles are checked before use. The mutex handle is
    // intentionally leaked (kept alive for process lifetime) for the primary instance.
    unsafe {
        let h_mutex = CreateMutexW(std::ptr::null_mut(), 0, mutex_name.as_ptr());
        if h_mutex.is_null() {
            eprintln!("CreateMutexW failed (skipping single instance detection)");
            return InstanceCheck::Primary;
        }

        let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;
        if !already_exists {
            // 最初のインスタンス — mutex ハンドルは意図的に close しない
            // （プロセス生存中保持、終了時に OS が解放）
            return InstanceCheck::Primary;
        }

        // 既存インスタンスあり — パイプ経由でファイルパスを送信
        CloseHandle(h_mutex); // 自分の mutex は不要

        let pipe_name = to_wide(PIPE_NAME);

        // パイプがまだ作成されていない場合に備えて最大3秒待機
        if WaitNamedPipeW(pipe_name.as_ptr(), 3000) == 0 {
            eprintln!("WaitNamedPipeW timeout (existing instance pipe not ready)");
            return InstanceCheck::FallbackStart;
        }

        let h_pipe = CreateFileW(
            pipe_name.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if h_pipe == INVALID_HANDLE_VALUE {
            eprintln!("Pipe connection failed (cannot send to existing instance)");
            return InstanceCheck::FallbackStart;
        }

        // メッセージモードに切り替え
        let mode = PIPE_READMODE_MESSAGE;
        SetNamedPipeHandleState(h_pipe, &mode, std::ptr::null_mut(), std::ptr::null_mut());

        // パスを絶対化して UTF-8 バイト列として送信
        let payload = match file_path {
            Some(p) => {
                let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
                let s = abs.to_string_lossy();
                // \\?\UNC\server\share → \\server\share, \\?\C:\... → C:\...
                let s = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
                    std::borrow::Cow::Owned(format!(r"\\{rest}"))
                } else if let Some(rest) = s.strip_prefix(r"\\?\") {
                    std::borrow::Cow::Borrowed(rest)
                } else {
                    s
                };
                s.as_bytes().to_vec()
            }
            None => Vec::new(),
        };

        let mut written: u32 = 0;
        let ok = WriteFile(
            h_pipe,
            payload.as_ptr(),
            payload.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        );
        CloseHandle(h_pipe);

        if ok == 0 || written != payload.len() as u32 {
            eprintln!(
                "Pipe write failed (written={written}, expected={})",
                payload.len()
            );
            return InstanceCheck::FallbackStart;
        }

        eprintln!(
            "Forwarded path to existing instance: {}",
            String::from_utf8_lossy(&payload)
        );
        InstanceCheck::Forwarded
    }
}

/// パイプリッスンスレッドを起動。受信したパスを sender に送る。
pub fn start_pipe_listener(sender: mpsc::Sender<PathBuf>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let pipe_name = to_wide(PIPE_NAME);
        loop {
            // SAFETY: pipe_name is a valid null-terminated UTF-16 string from to_wide().
            // All numeric parameters are valid pipe configuration constants.
            let h_pipe = unsafe {
                CreateNamedPipeW(
                    pipe_name.as_ptr(),
                    PIPE_ACCESS_INBOUND,
                    PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                    1,     // 最大インスタンス数
                    65536, // 出力バッファ（長パス対応で 64KB に拡大）
                    65536, // 入力バッファ
                    0,
                    std::ptr::null_mut(),
                )
            };
            let pipe = match WinHandle::new(h_pipe) {
                Some(p) => p,
                None => {
                    log::warn!("CreateNamedPipeW failed, listener stopped");
                    break;
                }
            };

            // クライアント接続待ち（ブロッキング）
            // SAFETY: pipe is a valid named pipe handle (checked in WinHandle::new).
            let connected = unsafe { ConnectNamedPipe(pipe.as_raw(), std::ptr::null_mut()) };
            if connected == 0 {
                // ERROR_PIPE_CONNECTED (535) は既に接続済みなので正常
                // SAFETY: GetLastError has no preconditions.
                let err = unsafe { GetLastError() };
                if err != 535 {
                    log::warn!("ConnectNamedPipe failed: error={err}");
                    // pipe は Drop で自動 CloseHandle
                    continue;
                }
            }

            // メッセージ読み取り（ERROR_MORE_DATA 時はループで全データを取得）
            let mut accumulated = Vec::new();
            let mut buf = [0u8; 65536];
            let mut read_ok = false;
            loop {
                let mut bytes_read: u32 = 0;
                // SAFETY: pipe is a valid connected pipe handle, buf is a stack-allocated
                // array with known size, and bytes_read is a valid mutable pointer.
                let ok = unsafe {
                    ReadFile(
                        pipe.as_raw(),
                        buf.as_mut_ptr(),
                        buf.len() as u32,
                        &mut bytes_read,
                        std::ptr::null_mut(),
                    )
                };
                if bytes_read > 0 {
                    accumulated.extend_from_slice(&buf[..bytes_read as usize]);
                }
                if ok != 0 {
                    read_ok = true; // メッセージ全体を正常に受信完了
                    break;
                }
                // SAFETY: GetLastError has no preconditions.
                let err = unsafe { GetLastError() };
                if err == ERROR_MORE_DATA {
                    // メッセージの続きがある — 次の ReadFile で残りを取得
                    continue;
                }
                // その他のエラーは回復不能 — 部分データは破棄
                log::warn!(
                    "ReadFile failed: error={err}, discarding {} partial bytes",
                    accumulated.len()
                );
                break;
            }

            if read_ok && !accumulated.is_empty() {
                let s = String::from_utf8_lossy(&accumulated);
                let path = PathBuf::from(s.into_owned());
                let _ = sender.send(path);
            } else if read_ok {
                // 空メッセージ = 前面化のみ
                let _ = sender.send(PathBuf::new());
            }

            ctx.request_repaint();

            // SAFETY: pipe is a valid pipe handle. DisconnectNamedPipe disconnects the
            // server end. CloseHandle is called automatically by WinHandle::drop.
            unsafe {
                DisconnectNamedPipe(pipe.as_raw());
            }
            // pipe は Drop で自動 CloseHandle
        }
    });
}
