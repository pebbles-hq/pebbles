//! Proc-macros for Pebbles. Currently just [`macro@component`] — the ergonomic authoring
//! form (F1). Re-exported from `pebbles::prelude`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, Pat, PatType, parse_macro_input};

/// Turn a plain function into a Pebbles component. THE authoring form (the underlying
/// `component_props` stays the documented mechanism, not
/// a second style).
///
/// ```ignore
/// #[component]
/// fn stat_card(title: String, value: i64) -> Element {
///     // hooks + the args (owned, by value) are available directly
///     column(children![text(title), text(format!("{value}"))]).into_widget()
/// }
/// // stat_card("Revenue".into(), 42) — call it like any widget ctor.
/// ```
///
/// Expands to a `Clone` props struct, a render `fn(&Props) -> Element`, and a public
/// ctor `fn name(args…) -> Element`. Rules: every arg is required, becomes an owned
/// `Clone` props field, and is handed to the body **by value** (a clone). Generics,
/// lifetimes and `self` are rejected with a clean error.
#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);

    if !func.sig.generics.params.is_empty() {
        return err(&func.sig.generics, "#[component] does not support generics or lifetimes");
    }

    let vis = &func.vis;
    let name = &func.sig.ident;
    let ret = &func.sig.output; // includes the `-> …`
    let body = &func.block;

    let mut arg_names = Vec::new();
    let mut arg_types = Vec::new();
    for input in &func.sig.inputs {
        match input {
            FnArg::Typed(PatType { pat, ty, .. }) => match &**pat {
                Pat::Ident(pi) => {
                    arg_names.push(pi.ident.clone());
                    arg_types.push((**ty).clone());
                }
                other => return err(other, "#[component] arguments must be plain identifiers"),
            },
            FnArg::Receiver(r) => return err(r, "#[component] cannot take `self`"),
        }
    }

    let props_ident = format_ident!("{}Props", to_pascal(&name.to_string()));
    let render_ident = format_ident!("__pebbles_{}_render", name);

    quote! {
        #[derive(Clone)]
        #vis struct #props_ident {
            #( pub #arg_names: #arg_types, )*
        }

        fn #render_ident(__props: & #props_ident) #ret {
            // Args are handed to the body by value (a clone of each field).
            #( let #arg_names = ::core::clone::Clone::clone(&__props.#arg_names); )*
            #body
        }

        #vis fn #name ( #( #arg_names: #arg_types ),* ) #ret {
            ::pebbles::core::IntoWidget::into_widget(::pebbles::core::component_props(
                #render_ident,
                #props_ident { #( #arg_names ),* },
            ))
        }
    }
    .into()
}

/// Mark the app entry point so it runs on every platform — Flutter's `void main()`.
///
/// It leaves your function **exactly as written** (so it's the ordinary `fn main`
/// on desktop/web, or a `pub fn run` your desktop bin calls) and *additionally*, on
/// **Android**, generates the `android_main(app: AndroidApp)` the OS calls there:
/// it stashes the `AndroidApp` so `App::run` can build the winit event loop with
/// it, then invokes your function.
///
/// Desktop/web — one file:
/// ```ignore
/// #[pebbles::main]
/// fn main() -> Result<(), Box<dyn std::error::Error>> { App::new(component(app)).run() }
/// ```
///
/// Cross-platform incl. Android — put it on `run()` in `lib.rs` (a `cdylib`), with a
/// thin `main.rs` calling `my_app::run()` for desktop (see documentations/android-support.md):
/// ```ignore
/// #[pebbles::main]
/// pub fn run() -> Result<(), Box<dyn std::error::Error>> { App::new(component(app)).run() }
/// ```
///
/// Adding it changes nothing off Android.
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    if !func.sig.generics.params.is_empty() {
        return err(&func.sig.generics, "#[pebbles::main] does not support generics");
    }
    let entry = &func.sig.ident; // the fn to call from android_main (usually `main` or `run`)

    quote! {
        // Your function, verbatim — the desktop/web entry (if named `main`) or a
        // `run()` your desktop bin calls.
        #func

        // Android: the OS calls `android_main`, not your fn. Stash the AndroidApp for
        // `App::run` to build the event loop with, then invoke your fn.
        #[cfg(target_os = "android")]
        #[unsafe(no_mangle)]
        extern "C" fn android_main(app: ::pebbles::shell::AndroidApp) {
            ::pebbles::shell::__set_android_app(app);
            let _ = #entry();
        }
    }
    .into()
}

fn err(tokens: impl quote::ToTokens, msg: &str) -> TokenStream {
    syn::Error::new_spanned(tokens, msg).to_compile_error().into()
}

/// `stat_card` → `StatCard`.
fn to_pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
