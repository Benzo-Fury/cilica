//! Windows: active capture sessions on the default microphone (IAudioSessionManager2 through
//! the `wasapi` crate), mapped to executable names. Type-checked only; not run on Windows yet.

use crate::MicUser;

fn exe_name(pid: u32) -> Option<String> {
    use ::windows::Win32::Foundation::CloseHandle;
    use ::windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), ::windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_owned)
    }
}

pub(crate) fn mic_users(_read_window_titles: bool) -> Vec<MicUser> {
    let result = (|| -> Result<Vec<MicUser>, Box<dyn std::error::Error>> {
        let _ = wasapi::initialize_mta();
        let enumerator = wasapi::DeviceEnumerator::new()?;
        let device = enumerator.get_default_device(wasapi::Direction::Capture)?;
        let manager = device.get_iaudiosessionmanager()?;
        let sessions = manager.get_audiosessionenumerator()?;
        let mut out = Vec::new();
        for i in 0..sessions.get_count()? {
            let session = sessions.get_session(i)?;
            if session.get_state()? != wasapi::SessionState::Active {
                continue;
            }
            let pid = session.get_process_id()?;
            if pid == 0 {
                continue;
            }
            if let Some(id) = exe_name(pid) {
                out.push(MicUser { pid, id, window_titles: Vec::new() });
            }
        }
        Ok(out)
    })();
    result.unwrap_or_else(|e| {
        tracing::debug!(error = %e, "could not list capture sessions");
        Vec::new()
    })
}
