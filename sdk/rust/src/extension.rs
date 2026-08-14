//! Extension lifecycle trait and export macro.

use crate::bindings::shilpo::extension::events::ExtensionEvent;
use crate::bindings::shilpo::extension::types::{Activation, DeactivateReason, Error};
use crate::bindings::shilpo::extension::view::ViewTree;

/// High-level extension lifecycle trait.
///
/// Extension authors implement this trait and register their implementation
/// with [`export_extension!`].
pub trait Extension: Default {
    /// Called when the extension component is activated by the host.
    fn activate(&mut self, activation: Activation) -> Result<(), Error> {
        let _ = activation;
        Ok(())
    }

    /// Called when the extension component is being deactivated.
    fn deactivate(&mut self, reason: DeactivateReason) -> Result<(), Error> {
        let _ = reason;
        Ok(())
    }

    /// Called when an inbound event occurs (lifecycle, user input, state change, etc.).
    fn on_event(&mut self, event: ExtensionEvent) -> Result<(), Error> {
        let _ = event;
        Ok(())
    }

    /// Called by the host to render a declarative ViewTree for a UI contribution.
    fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
        let _ = contribution_id;
        Ok(None)
    }
}

/// Exports an [`Extension`] implementation to the canonical WebAssembly guest boundary.
///
/// # Examples
///
/// ```rust,no_run
/// use shilpo_ext_sdk::prelude::*;
///
/// #[derive(Default)]
/// struct MyExtension;
///
/// impl Extension for MyExtension {
///     fn view(&mut self, contribution_id: &str) -> Result<Option<ViewTree>, Error> {
///         if contribution_id == "widget" {
///             Ok(Some(view! {
///                 row() {
///                     text("Hello World!"),
///                 }
///             }))
///         } else {
///             Ok(None)
///         }
///     }
/// }
///
/// export_extension!(MyExtension);
/// ```
#[macro_export]
macro_rules! export_extension {
    ($ext:ty) => {
        #[allow(unused_imports)]
        use $crate::bindings::export;

        struct __ShilpoGuestComponent;

        static mut __SHILPO_INSTANCE: ::core::option::Option<$ext> = ::core::option::Option::None;

        impl $crate::bindings::Guest for __ShilpoGuestComponent {
            fn activate(
                activation: $crate::bindings::shilpo::extension::types::Activation,
            ) -> ::core::result::Result<(), $crate::bindings::shilpo::extension::types::Error> {
                let inst = unsafe {
                    if __SHILPO_INSTANCE.is_none() {
                        __SHILPO_INSTANCE = ::core::option::Option::Some(<$ext as ::core::default::Default>::default());
                    }
                    __SHILPO_INSTANCE.as_mut().unwrap()
                };
                <$ext as $crate::extension::Extension>::activate(inst, activation)
            }

            fn deactivate(
                reason: $crate::bindings::shilpo::extension::types::DeactivateReason,
            ) -> ::core::result::Result<(), $crate::bindings::shilpo::extension::types::Error> {
                let inst = unsafe {
                    if __SHILPO_INSTANCE.is_none() {
                        __SHILPO_INSTANCE = ::core::option::Option::Some(<$ext as ::core::default::Default>::default());
                    }
                    __SHILPO_INSTANCE.as_mut().unwrap()
                };
                let res = <$ext as $crate::extension::Extension>::deactivate(inst, reason);
                unsafe {
                    __SHILPO_INSTANCE = ::core::option::Option::None;
                }
                res
            }

            fn on_event(
                event: $crate::bindings::shilpo::extension::events::ExtensionEvent,
            ) -> ::core::result::Result<(), $crate::bindings::shilpo::extension::types::Error> {
                let inst = unsafe {
                    if __SHILPO_INSTANCE.is_none() {
                        __SHILPO_INSTANCE = ::core::option::Option::Some(<$ext as ::core::default::Default>::default());
                    }
                    __SHILPO_INSTANCE.as_mut().unwrap()
                };
                <$ext as $crate::extension::Extension>::on_event(inst, event)
            }

            fn view(
                contribution_id: ::std::string::String,
            ) -> ::core::result::Result<
                ::core::option::Option<$crate::bindings::shilpo::extension::view::ViewTree>,
                $crate::bindings::shilpo::extension::types::Error,
            > {
                let inst = unsafe {
                    if __SHILPO_INSTANCE.is_none() {
                        __SHILPO_INSTANCE = ::core::option::Option::Some(<$ext as ::core::default::Default>::default());
                    }
                    __SHILPO_INSTANCE.as_mut().unwrap()
                };
                <$ext as $crate::extension::Extension>::view(inst, &contribution_id)
            }
        }

        $crate::bindings::export!(__ShilpoGuestComponent with_types_in $crate::bindings);
    };
}
