//! Desktop banner notifications — the sound.rs approach applied to banners
//! (platform facilities, zero new Rust deps, failures swallowed):
//!
//! - macOS: `NSUserNotification` through the already-linked ObjC runtime, so
//!   the banner is attributed to Zeron with its icon. The API is deprecated
//!   (10.14) but shipping and fits an app that needs no actions/attachments;
//!   a delegate answers "present" unconditionally, overriding the center's
//!   own suppress-while-frontmost policy — WHETHER to ping (including the
//!   background-only setting's focus check) is decided at the call site, so
//!   every platform path behaves identically. Unbundled dev runs
//!   (`cargo run`) have no notification
//!   center — those adopt the installed Zeron app's bundle identity (the
//!   terminal-notifier technique: override `-[NSBundle bundleIdentifier]` for
//!   the main bundle), so dev banners still show the Zeron icon; a click may
//!   focus the installed app rather than the dev binary — acceptable for a
//!   dev-only path. Machines without Zeron installed fall back to
//!   `osascript`, attributed to Script Editor (cosmetics only).
//! - Linux: `notify-send` (libnotify's CLI, present on every mainstream
//!   desktop).
//! - Windows: no-op for now — toasts require a registered AppUserModelID
//!   (an installer concern); the chime still covers it.
//! - `POSTILLION_DISABLE_NOTIFICATIONS` env kill-switch + the
//!   `notificationsEnabled` ui-setting (checked by the caller);
//! - failures are logged and swallowed — a missing notifier must never
//!   bother the session flow.

const DISABLE_ENV: &str = "POSTILLION_DISABLE_NOTIFICATIONS";

/// Post a desktop banner. Call from the main thread (the macOS native path
/// talks to AppKit); slow paths (spawning a CLI) hop to a background thread.
/// Silently a no-op when disabled or no notifier is available.
pub fn post(title: &str, body: &str) {
    if std::env::var_os(DISABLE_ENV).is_some() {
        return;
    }
    post_impl(title, body);
}

#[cfg(target_os = "macos")]
fn post_impl(title: &str, body: &str) {
    if post_user_notification(title, body) {
        return;
    }
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(body),
        applescript_escape(title),
    );
    std::thread::spawn(move || {
        let result = std::process::Command::new("osascript")
            .args(["-e", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match result {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::debug!(?status, "osascript notification failed"),
            Err(err) => tracing::debug!(error = %err, "osascript unavailable"),
        }
    });
}

/// The identity banners are attributed to — the packaged app's bundle id
/// (`dist/macos/Info.plist`), which the center resolves to its name + icon.
#[cfg(target_os = "macos")]
const MACOS_BUNDLE_ID: &std::ffi::CStr = c"sh.postillion.app";

/// Deliver through the app's notification center; false when the process has
/// no bundle (dev runs — `defaultUserNotificationCenter` is nil there) and
/// the installed-app identity can't be adopted either.
#[cfg(target_os = "macos")]
fn post_user_notification(title: &str, body: &str) -> bool {
    use objc::runtime::{Class, Object};
    use objc::{class, msg_send, sel, sel_impl};
    // Defensive lookup (not `class!`, which panics if the class is ever
    // dropped from Foundation) — the deprecated API's one real removal risk.
    let Some(center_class) = Class::get("NSUserNotificationCenter") else {
        return false;
    };
    let (Ok(title), Ok(body)) = (
        std::ffi::CString::new(title.replace('\0', "")),
        std::ffi::CString::new(body.replace('\0', "")),
    ) else {
        return false;
    };
    unsafe {
        let mut center: *mut Object = msg_send![center_class, defaultUserNotificationCenter];
        if center.is_null() && identity::adopt_installed() {
            center = msg_send![center_class, defaultUserNotificationCenter];
        }
        if center.is_null() {
            return false;
        }
        // Idempotent: the delegate is a leaked singleton (the center holds it
        // weakly), re-set on every post in case another center appeared.
        let _: () = msg_send![center, setDelegate: delegate::always_present()];
        let note: *mut Object = msg_send![class!(NSUserNotification), new];
        let ns_title: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: title.as_ptr()];
        let _: () = msg_send![note, setTitle: ns_title];
        let ns_body: *mut Object = msg_send![class!(NSString), stringWithUTF8String: body.as_ptr()];
        let _: () = msg_send![note, setInformativeText: ns_body];
        let _: () = msg_send![center, deliverNotification: note];
        let _: () = msg_send![note, release];
    }
    true
}

/// The center delegate: `shouldPresentNotification:` → YES, unconditionally.
/// Without it the center silently swallows banners while the app is
/// frontmost — a policy that lived outside our settings; presenting always
/// moves the whole decision into the caller (`shell::on_state_changed`
/// checks `notifications_background_only` + window focus before posting).
#[cfg(target_os = "macos")]
mod delegate {
    use std::sync::OnceLock;

    use objc::declare::ClassDecl;
    use objc::runtime::{BOOL, Object, Sel, YES};
    use objc::{class, msg_send, sel, sel_impl};

    extern "C" fn should_present(
        _this: &Object,
        _sel: Sel,
        _center: *mut Object,
        _notification: *mut Object,
    ) -> BOOL {
        YES
    }

    /// The leaked delegate singleton (as usize — raw pointers aren't `Sync`).
    /// Leaked deliberately: the center's `delegate` property is weak.
    pub(super) fn always_present() -> *mut Object {
        static DELEGATE: OnceLock<usize> = OnceLock::new();
        *DELEGATE.get_or_init(|| unsafe {
            let mut decl = ClassDecl::new("PostillionNotifyDelegate", class!(NSObject))
                .expect("PostillionNotifyDelegate registered twice");
            decl.add_method(
                sel!(userNotificationCenter:shouldPresentNotification:),
                should_present as extern "C" fn(&Object, Sel, *mut Object, *mut Object) -> BOOL,
            );
            let class = decl.register();
            let instance: *mut Object = msg_send![class, new];
            instance as usize
        }) as *mut Object
    }
}

/// Bundle-identity adoption for unbundled (dev) processes — the
/// terminal-notifier / mac-notification-sys technique. The notification
/// center refuses processes whose main bundle has no identifier and resolves
/// each banner's name + icon from the identifier at delivery, so overriding
/// `-[NSBundle bundleIdentifier]` to answer the installed Zeron app's id for
/// the MAIN bundle (other bundles keep the original implementation) makes
/// dev-run banners look exactly like the packaged app's.
#[cfg(target_os = "macos")]
mod identity {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use objc::runtime::{Class, Imp, Method, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe extern "C" {
        fn class_getInstanceMethod(cls: *const Class, name: Sel) -> *mut Method;
        fn method_getImplementation(method: *const Method) -> Imp;
        fn method_setImplementation(method: *mut Method, imp: Imp) -> Imp;
    }

    /// The pre-override `bundleIdentifier` IMP (as usize — fn pointers have
    /// no atomic type), forwarded to for every bundle but the main one.
    static ORIGINAL: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn bundle_identifier_override(this: &Object, sel: Sel) -> *mut Object {
        unsafe {
            let main: *mut Object = msg_send![class!(NSBundle), mainBundle];
            if std::ptr::eq(this, main) {
                return msg_send![
                    class!(NSString),
                    stringWithUTF8String: super::MACOS_BUNDLE_ID.as_ptr()
                ];
            }
            match ORIGINAL.load(Ordering::Relaxed) {
                0 => std::ptr::null_mut(),
                imp => {
                    let original: extern "C" fn(&Object, Sel) -> *mut Object =
                        std::mem::transmute(imp);
                    original(this, sel)
                }
            }
        }
    }

    /// Install the override once; false when the identity can't (or needn't)
    /// be adopted. Only ever called after `defaultUserNotificationCenter`
    /// returned nil, i.e. never in the packaged app.
    pub(super) fn adopt_installed() -> bool {
        static ADOPTED: OnceLock<bool> = OnceLock::new();
        *ADOPTED.get_or_init(|| unsafe {
            // Adoption only works if the system can resolve the id to an
            // installed app — otherwise stay on the osascript fallback.
            let bundle_id: *mut Object = msg_send![
                class!(NSString),
                stringWithUTF8String: super::MACOS_BUNDLE_ID.as_ptr()
            ];
            let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
            let url: *mut Object =
                msg_send![workspace, URLForApplicationWithBundleIdentifier: bundle_id];
            if url.is_null() {
                return false;
            }
            let method = class_getInstanceMethod(class!(NSBundle), sel!(bundleIdentifier));
            if method.is_null() {
                return false;
            }
            ORIGINAL.store(method_getImplementation(method) as usize, Ordering::Relaxed);
            let replacement: extern "C" fn(&Object, Sel) -> *mut Object =
                bundle_identifier_override;
            method_setImplementation(method, std::mem::transmute::<_, Imp>(replacement));
            true
        })
    }
}

/// Escape a string for interpolation inside a double-quoted AppleScript
/// literal: backslash and double-quote are the metacharacters, and a raw
/// newline ends the statement (which would silently drop the banner) — those
/// flatten to spaces.
#[cfg(any(target_os = "macos", test))]
fn applescript_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}

#[cfg(target_os = "linux")]
fn post_impl(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        // `--` ends option parsing: session titles are model-generated, so a
        // `-`-leading one must land as the summary, not as a flag.
        let result = std::process::Command::new("notify-send")
            .args(["--app-name=Postillion", "--", &title, &body])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match result {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::debug!(?status, "notify-send failed"),
            Err(err) => tracing::debug!(error = %err, "notify-send unavailable"),
        }
    });
}

/// XML metin düğümüne gömmek için kaçış. Başlıklar model üretimi olduğu için
/// `&`/`<` gerçekten geliyor; kaçmazsa toast XML'i parse edilemez ve bildirim
/// sessizce düşer. Tek tırnak PowerShell literalinde ikilenerek kaçılıyor.
#[cfg(any(target_os = "windows", test))]
fn toast_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            '\r' => {}
            '\n' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

/// Windows toast'ı, ek bağımlılık olmadan PowerShell üzerinden WinRT
/// `ToastNotificationManager` ile. AppId olarak PowerShell'in kayıtlı
/// kısayol kimliği kullanılıyor: kendi AUMID'imizi Start menüsüne kaydetmeden
/// bildirim gösterilemiyor, kurulum paketi bunu yaptığında burası
/// `Postillion` AUMID'ine geçecek.
#[cfg(target_os = "windows")]
fn post_impl(title: &str, body: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const APP_ID: &str =
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";
    let (title, body) = (toast_escape(title), toast_escape(body));
    std::thread::spawn(move || {
        let script = format!(
            // Her iki WinRT tipi de ayrı ayrı yüklenmeli: `New-Object` ancak
            // tip ContentType=WindowsRuntime ile çözüldükten sonra çalışıyor.
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType=WindowsRuntime] > $null; \
             [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType=WindowsRuntime] > $null; \
             $xml = New-Object Windows.Data.Xml.Dom.XmlDocument; \
             $xml.LoadXml('<toast><visual><binding template=\"ToastGeneric\"><text>{title}</text><text>{body}</text></binding></visual></toast>'); \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{APP_ID}').Show([Windows.UI.Notifications.ToastNotification]::new($xml))"
        );
        let result = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match result {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::debug!(?status, "toast failed"),
            Err(err) => tracing::debug!(error = %err, "powershell unavailable"),
        }
    });
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn post_impl(_title: &str, _body: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applescript_escaping() {
        assert_eq!(applescript_escape("plain"), "plain");
        assert_eq!(
            applescript_escape(r#"say "hi" \ bye"#),
            r#"say \"hi\" \\ bye"#
        );
        // Raw newlines would end the AppleScript statement mid-literal.
        assert_eq!(applescript_escape("two\nlines\r\n"), "two lines  ");
    }

    #[test]
    fn toast_escaping() {
        assert_eq!(toast_escape("plain"), "plain");
        // Kaçmayan `&`/`<` toast XML'ini bozar — bildirim hiç görünmez.
        assert_eq!(toast_escape("a & b < c"), "a &amp; b &lt; c");
        // Tek tırnak PowerShell literalinin içinde: XML entity'si güvenli.
        assert_eq!(toast_escape("it's"), "it&apos;s");
        assert_eq!(toast_escape("two\nlines\r\n"), "two lines ");
    }
}
