//! iOS file picker backend backed by `UIDocumentPickerViewController`.
//!
//! Mirrors [`super::ios_clipboard::IosClipboard`]: zero-sized struct,
//! main-thread only, no stored state. The picker's async result is
//! delivered via the `on_done` callback stashed in the delegate's ivars.
//!
//! # Thread safety
//!
//! `UIDocumentPickerViewController` and its delegate methods must be
//! invoked on the main thread. Every call site fires from winit's
//! main-loop event dispatch in [`crate::window`], so this invariant holds
//! without extra marshalling. The struct stores no state, so it is
//! trivially `Send + Sync` and can be shared as `Arc<dyn FilePicker>`.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSArray, NSObject, NSURL};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerDelegate, UIDocumentPickerViewController, UIViewController,
};
use objc2_uniform_type_identifiers::UTTypeItem;

use super::file_picker::{file_within_limit, mime_from_extension_str, FilePicker, PickedFile};

type PendingCallback = Rc<RefCell<Option<Box<dyn FnOnce(Option<PickedFile>)>>>>;

thread_local! {
    static LIVE_DELEGATE: RefCell<Option<Retained<NSObject>>> = const { RefCell::new(None) };
}

pub struct IosFilePicker;

impl FilePicker for IosFilePicker {
    fn pick_file(&self, on_done: Box<dyn FnOnce(Option<PickedFile>)>) {
        let mtm = match MainThreadMarker::new() {
            Some(mtm) => mtm,
            None => {
                on_done(None);
                return;
            }
        };
        let slot: PendingCallback = Rc::new(RefCell::new(Some(on_done)));

        let content_types = unsafe { NSArray::from_slice(&[UTTypeItem]) };
        let picker = UIDocumentPickerViewController::initForOpeningContentTypes(
            UIDocumentPickerViewController::alloc(mtm),
            &content_types,
        );
        picker.setAllowsMultipleSelection(false);

        let delegate = DocumentPickerDelegate::new(slot, mtm);
        let delegate_obj: Retained<NSObject> = unsafe { Retained::cast_unchecked(delegate) };
        LIVE_DELEGATE.with(|d| *d.borrow_mut() = Some(delegate_obj.clone()));

        let delegate_proto: Retained<objc2::runtime::ProtocolObject<dyn UIDocumentPickerDelegate>> =
            unsafe { Retained::cast_unchecked(delegate_obj) };
        picker.setDelegate(Some(&delegate_proto));

        if let Some(vc) = topmost_view_controller(&mtm) {
            let picker_vc: Retained<UIViewController> = unsafe { Retained::cast_unchecked(picker) };
            vc.presentViewController_animated_completion(&picker_vc, true, None);
        } else {
            LIVE_DELEGATE.with(|d| *d.borrow_mut() = None);
        }
    }
}

#[derive(Clone)]
struct DelegateIvars {
    callback: PendingCallback,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    struct DocumentPickerDelegate;

    unsafe impl NSObjectProtocol for DocumentPickerDelegate {}

    #[allow(non_snake_case)]
    unsafe impl UIDocumentPickerDelegate for DocumentPickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn documentPicker_didPickDocumentsAtURLs(
            &self,
            _controller: &UIDocumentPickerViewController,
            urls: &NSArray<NSURL>,
        ) {
            let picked = urls.firstObject().and_then(|url| read_url(&url));
            self.fire(picked);
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn documentPickerWasCancelled(&self, _controller: &UIDocumentPickerViewController) {
            self.fire(None);
        }
    }
);

impl DocumentPickerDelegate {
    fn new(slot: PendingCallback, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars { callback: slot });
        unsafe { msg_send![super(this), init] }
    }

    fn fire(&self, picked: Option<PickedFile>) {
        if let Some(cb) = self.ivars().callback.borrow_mut().take() {
            cb(picked);
        }
        LIVE_DELEGATE.with(|d| *d.borrow_mut() = None);
    }
}

struct SecurityScopeGuard<'a> {
    url: &'a NSURL,
    acquired: bool,
}

impl<'a> SecurityScopeGuard<'a> {
    fn new(url: &'a NSURL) -> Self {
        let acquired = unsafe { url.startAccessingSecurityScopedResource() };
        Self { url, acquired }
    }
}

impl<'a> Drop for SecurityScopeGuard<'a> {
    fn drop(&mut self) {
        if self.acquired {
            unsafe { self.url.stopAccessingSecurityScopedResource() };
        }
    }
}

fn read_url(url: &NSURL) -> Option<PickedFile> {
    let _guard = SecurityScopeGuard::new(url);
    let path = url.path()?;
    let path_str = path.to_string();
    let std_path = std::path::Path::new(&path_str);
    let metadata = std::fs::metadata(std_path).ok()?;
    if !file_within_limit(metadata.len()) {
        return None;
    }
    let bytes = std::fs::read(std_path).ok()?;
    let name = url
        .lastPathComponent()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into());
    let ext = url
        .pathExtension()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let mime = mime_from_extension_str(&ext.to_lowercase());
    Some(PickedFile { name, mime, bytes })
}

fn topmost_view_controller(mtm: &MainThreadMarker) -> Option<Retained<UIViewController>> {
    let app = UIApplication::sharedApplication(*mtm);
    let key_window = app.keyWindow()?;
    let mut vc = key_window.rootViewController()?;
    while let Some(presented) = vc.presentedViewController() {
        vc = presented;
    }
    Some(vc)
}
