//! Sends the notification of new folder offers (text decided by `crate::notification`).
//!
//! `SceNotificationUtil` is the API applications use to post to the LiveArea notification
//! list; the worker runs inside `SceShell`, which owns that list, and calls it from its own
//! thread. `sceNotificationUtilBgAppInitialize` is deliberately not called: it is only for
//! background-application processes (psp2/notificationutil.h), and `SceShell` is not one.
//! A failure is logged and never fails the cycle.

use libc::c_int;

use super::ffi;
use crate::log::WorkerLog;
use crate::notification::{notification_text, utf16_buffer, NotifyLanguage};
use crate::offers::NewOffer;

/// System language code from the registry (`/CONFIG/SYSTEM/language`), `None` if unreadable.
fn system_language_code() -> Option<i32> {
    let mut code: c_int = 0;
    let ret = unsafe {
        ffi::sceRegMgrGetKeyInt(c"/CONFIG/SYSTEM".as_ptr(), c"language".as_ptr(), &mut code)
    };
    (ret >= 0).then_some(code)
}

/// Posts one notification for the offers this cycle recorded (nothing without any).
pub fn notify_new_offers(new_offers: &[NewOffer], log: &WorkerLog) {
    let lang = NotifyLanguage::from_system_code(system_language_code());
    let Some(text) = notification_text(lang, new_offers) else {
        return;
    };
    // The weak import is only bound if the library was loaded before the worker.
    if unsafe { ffi::sceSysmoduleIsLoaded(ffi::SCE_SYSMODULE_NOTIFICATION_UTIL) } != 0 {
        log.line("notification skipped: SceNotificationUtil is not loaded");
        return;
    }
    let buf = utf16_buffer(&text);
    let ret = unsafe { ffi::sceNotificationUtilSendNotification(buf.as_ptr()) };
    if ret < 0 {
        log.line(&format!("notification failed (0x{:08X})", ret as u32));
    }
}
