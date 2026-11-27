//! 跨平台敏感凭证剪贴板写入：尽可能阻止剪贴板历史、云同步与跨设备同步记录令牌。

#[cfg(windows)]
mod platform {
    use std::time::Duration;
    use std::{mem, ptr, thread};
    use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL, HWND};
    use windows_sys::Win32::System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW,
            SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    };

    const CF_UNICODETEXT: u32 = 13;
    const COPY_FAILED: &str = "复制令牌失败，可从令牌文件手动复制";
    const CLIPBOARD_BUSY: &str = "剪贴板正被占用，请稍后再试";
    const OPEN_CLIPBOARD_ATTEMPTS: usize = 10;
    const OPEN_CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(15);

    pub(crate) fn copy_secret(owner: HWND, secret: &str) -> Result<(), &'static str> {
        let exclude_format = register_format(windows_sys::core::w!(
            "ExcludeClipboardContentFromMonitorProcessing"
        ))?;
        let history_format =
            register_format(windows_sys::core::w!("CanIncludeInClipboardHistory"))?;
        let cloud_format = register_format(windows_sys::core::w!("CanUploadClipboardData"))?;

        // Microsoft's clipboard history / cloud sync spec expects a 4-byte DWORD value of 0.
        let zero_dword = 0_u32.to_ne_bytes();
        let exclude = ClipboardMemory::from_bytes(&zero_dword)?;
        let no_history = ClipboardMemory::from_bytes(&zero_dword)?;
        let no_cloud = ClipboardMemory::from_bytes(&zero_dword)?;
        let utf16: Vec<u16> = secret.encode_utf16().chain([0]).collect();
        let text = ClipboardMemory::from_utf16(&utf16)?;

        // Other apps (Clipboard History, screenshot tools, RDP, WebView2 right after copying
        // instructions) may briefly lock the clipboard; retry with a short backoff.
        let _clipboard = open_clipboard_with_retry(owner)?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(COPY_FAILED);
        }
        // Publish privacy markers before publishing the secret text.
        exclude.publish(exclude_format)?;
        no_history.publish(history_format)?;
        no_cloud.publish(cloud_format)?;
        text.publish(CF_UNICODETEXT)
    }

    fn register_format(name: windows_sys::core::PCWSTR) -> Result<u32, &'static str> {
        let format = unsafe { RegisterClipboardFormatW(name) };
        if format == 0 {
            Err(COPY_FAILED)
        } else {
            Ok(format)
        }
    }

    fn open_clipboard_with_retry(owner: HWND) -> Result<ClipboardGuard, &'static str> {
        for attempt in 0..OPEN_CLIPBOARD_ATTEMPTS {
            // The main window owns the clipboard until the guard closes it.
            if unsafe { OpenClipboard(owner) } != 0 {
                return Ok(ClipboardGuard);
            }
            if attempt + 1 < OPEN_CLIPBOARD_ATTEMPTS {
                thread::sleep(OPEN_CLIPBOARD_RETRY_DELAY);
            }
        }
        Err(CLIPBOARD_BUSY)
    }

    struct ClipboardGuard;

    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }

    struct ClipboardMemory(HGLOBAL);

    impl ClipboardMemory {
        fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
            // SetClipboardData requires movable memory for these formats.
            let data = Self(unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) });
            if data.0.is_null() {
                return Err(COPY_FAILED);
            }
            let target = unsafe { GlobalLock(data.0) }.cast::<u8>();
            if target.is_null() {
                return Err(COPY_FAILED);
            }
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
                GlobalUnlock(data.0);
            }
            Ok(data)
        }

        fn from_utf16(text: &[u16]) -> Result<Self, &'static str> {
            let data = Self(unsafe { GlobalAlloc(GMEM_MOVEABLE, mem::size_of_val(text)) });
            if data.0.is_null() {
                return Err(COPY_FAILED);
            }
            let target = unsafe { GlobalLock(data.0) }.cast::<u16>();
            if target.is_null() {
                return Err(COPY_FAILED);
            }
            unsafe {
                ptr::copy_nonoverlapping(text.as_ptr(), target, text.len());
                GlobalUnlock(data.0);
            }
            Ok(data)
        }

        fn publish(mut self, format: u32) -> Result<(), &'static str> {
            if unsafe { SetClipboardData(format, self.0) }.is_null() {
                return Err(COPY_FAILED);
            }
            // Ownership transfers to Windows only after a successful call.
            self.0 = ptr::null_mut();
            Ok(())
        }
    }

    impl Drop for ClipboardMemory {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { GlobalFree(self.0) };
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{
        NSPasteboard, NSPasteboardContentsOptions, NSPasteboardItem, NSPasteboardTypeString,
        NSPasteboardWriting,
    };
    use objc2_foundation::{NSArray, NSData, NSString};

    const COPY_FAILED: &str = "复制令牌失败，可从令牌文件手动复制";
    // Standard macOS pasteboard markers honored by clipboard managers (Raycast, Maccy, Paste,
    // Alfred, CleanClip, etc.) to keep sensitive credentials out of persistent history.
    const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";
    const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";

    pub(crate) fn copy_secret(secret: &str) -> Result<(), &'static str> {
        let pasteboard = NSPasteboard::generalPasteboard();
        write_secret_to_pasteboard(&pasteboard, secret)
    }

    fn write_secret_to_pasteboard(
        pasteboard: &NSPasteboard,
        secret: &str,
    ) -> Result<(), &'static str> {
        // CurrentHostOnly prevents Universal Clipboard / Handoff from syncing the secret over iCloud.
        pasteboard.prepareForNewContentsWithOptions(NSPasteboardContentsOptions::CurrentHostOnly);

        let item = NSPasteboardItem::new();
        let empty = NSData::new();
        let concealed = NSString::from_str(CONCEALED_TYPE);
        let transient = NSString::from_str(TRANSIENT_TYPE);
        let value = NSString::from_str(secret);
        // SAFETY: NSPasteboardTypeString is an immutable AppKit global string constant.
        let string_type = unsafe { NSPasteboardTypeString };

        let prepared = item.setData_forType(&empty, &concealed)
            && item.setData_forType(&empty, &transient)
            && item.setString_forType(&value, string_type);
        if !prepared {
            return Err(COPY_FAILED);
        }

        let writable = ProtocolObject::<dyn NSPasteboardWriting>::from_retained(item);
        let objects = NSArray::from_retained_slice(&[writable]);
        if !pasteboard.writeObjects(&objects) {
            return Err(COPY_FAILED);
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn macos_pasteboard_writes_secret_with_privacy_markers() {
            // pasteboardWithUniqueName isolates the test from the user's real clipboard.
            let pasteboard = NSPasteboard::pasteboardWithUniqueName();
            let secret = "muse_agent_test_token_1234567890";
            write_secret_to_pasteboard(&pasteboard, secret).unwrap();

            let string_type = unsafe { NSPasteboardTypeString };
            let copied = pasteboard
                .stringForType(string_type)
                .expect("secret text should be present on pasteboard");
            assert_eq!(copied.to_string(), secret);

            let concealed = NSString::from_str(CONCEALED_TYPE);
            let transient = NSString::from_str(TRANSIENT_TYPE);
            assert!(
                pasteboard.dataForType(&concealed).is_some(),
                "concealed privacy marker should be present"
            );
            assert!(
                pasteboard.dataForType(&transient).is_some(),
                "transient privacy marker should be present"
            );

            pasteboard.clearContents();
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use std::io::{self, Write};
    use std::process::{Command, Stdio};

    const COPY_FAILED: &str = "复制令牌失败，可从令牌文件手动复制";
    const MISSING_TOOL: &str =
        "未找到系统剪贴板工具（如 wl-copy / xclip / xsel），请从令牌文件手动复制";

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ClipboardCommand {
        program: &'static str,
        args: &'static [&'static str],
    }

    const WL_COPY: ClipboardCommand = ClipboardCommand {
        program: "wl-copy",
        args: &[],
    };
    const XCLIP: ClipboardCommand = ClipboardCommand {
        program: "xclip",
        args: &["-selection", "clipboard", "-in"],
    };
    const XSEL: ClipboardCommand = ClipboardCommand {
        program: "xsel",
        args: &["--clipboard", "--input"],
    };

    fn candidates(has_wayland: bool, has_x11: bool) -> Vec<ClipboardCommand> {
        match (has_wayland, has_x11) {
            (true, true) => vec![WL_COPY, XCLIP, XSEL],
            (true, false) => vec![WL_COPY, XCLIP, XSEL],
            (false, true) => vec![XCLIP, XSEL, WL_COPY],
            (false, false) => vec![WL_COPY, XCLIP, XSEL],
        }
    }

    fn run_clipboard_command(cmd: ClipboardCommand, secret: &str) -> io::Result<bool> {
        // Stdout and stderr must be null so backgrounded clipboard daemons (wl-copy / xclip)
        // never inherit open pipes that would block wait().
        let mut child = Command::new(cmd.program)
            .args(cmd.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(secret.as_bytes())?;
        }
        let status = child.wait()?;
        Ok(status.success())
    }

    pub(crate) fn copy_secret(secret: &str) -> Result<(), &'static str> {
        let has_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
        let has_x11 = std::env::var_os("DISPLAY").is_some_and(|v| !v.is_empty());
        let mut any_found = false;

        for cmd in candidates(has_wayland, has_x11) {
            match run_clipboard_command(cmd, secret) {
                Ok(true) => return Ok(()),
                Ok(false) => any_found = true,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(_) => any_found = true,
            }
        }

        if any_found {
            Err(COPY_FAILED)
        } else {
            Err(MISSING_TOOL)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn prefers_wayland_then_x11_clipboard_tools() {
            assert_eq!(candidates(true, true)[0].program, "wl-copy");
            assert_eq!(candidates(false, true)[0].program, "xclip");
            assert_eq!(candidates(false, true)[1].program, "xsel");
        }
    }
}

pub(crate) use platform::copy_secret;
