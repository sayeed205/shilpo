//! Declarative UI composition macro.

/// Constructs a canonical [`ViewTree`](crate::bindings::shilpo::extension::view::ViewTree)
/// using declarative, nested syntax.
///
/// # Examples
///
/// ```rust
/// use shilpo_ext_sdk::prelude::*;
///
/// let show_badge = true;
/// let items = vec!["Alpha", "Beta"];
///
/// let tree = view! {
///     column {
///         text("Overview").bold(true),
///         row {
///             icon("bell").size(16.0),
///             button("Action", "btn-event"),
///         },
///         if show_badge {
///             badge("Live"),
///         },
///         for item in (items) {
///             text(item),
///         },
///         divider(),
///     }
/// };
///
/// assert_eq!(tree.root, 0);
/// ```
#[macro_export]
macro_rules! view {
    // Container constructor with arguments and children block: e.g. row(gap = 8.0) { ... }
    ($constructor:ident ( $($arg:expr),* $(,)? ) { $($children:tt)* }) => {{
        #[allow(unused_mut)]
        let mut builder = $crate::builder::$constructor($($arg),*);
        $crate::__view_children!(builder, $($children)*);
        $crate::builder::build_view_tree(builder)
    }};

    // Bare constructor with children block: e.g. row { ... } or column { ... }
    ($constructor:ident { $($children:tt)* }) => {{
        #[allow(unused_mut)]
        let mut builder = $crate::builder::$constructor();
        $crate::__view_children!(builder, $($children)*);
        $crate::builder::build_view_tree(builder)
    }};

    // Single builder expression: e.g. view!(text("Hello"))
    ($expr:expr) => {
        $crate::builder::build_view_tree($expr)
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __view_children {
    // Empty cases
    ($builder:ident,) => {};
    ($builder:ident) => {};

    // Nested block with arguments: e.g. row(gap = 4.0) { ... }
    ($builder:ident, $tag:ident ( $($arg:expr),* $(,)? ) { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        {
            #[allow(unused_mut)]
            let mut child_builder = $crate::builder::$tag($($arg),*);
            $crate::__view_children!(child_builder, $($inner)*);
            $builder = $builder.child(child_builder);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // Nested bare block: e.g. row { ... }
    ($builder:ident, $tag:ident { $($inner:tt)* } $(, $($rest:tt)*)?) => {
        {
            #[allow(unused_mut)]
            let mut child_builder = $crate::builder::$tag();
            $crate::__view_children!(child_builder, $($inner)*);
            $builder = $builder.child(child_builder);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // For loop child iteration with parenthesized expr: for var in (iter) { ... }
    ($builder:ident, for $var:pat in ( $($iter:tt)+ ) { $($body:tt)* } $(, $($rest:tt)*)?) => {
        for $var in $($iter)+ {
            $crate::__view_children!($builder, $($body)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // For loop child iteration with identifier: for var in iter { ... }
    ($builder:ident, for $var:pat in $iter:ident { $($body:tt)* } $(, $($rest:tt)*)?) => {
        for $var in $iter {
            $crate::__view_children!($builder, $($body)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If-let conditional with else and parenthesized expr: if let pat = (expr) { ... } else { ... }
    ($builder:ident, if let $pat:pat = ( $($expr:tt)+ ) { $($then:tt)* } else { $($els:tt)* } $(, $($rest:tt)*)?) => {
        if let $pat = $($expr)+ {
            $crate::__view_children!($builder, $($then)*);
        } else {
            $crate::__view_children!($builder, $($els)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If-let conditional with else and ident: if let pat = expr { ... } else { ... }
    ($builder:ident, if let $pat:pat = $expr:ident { $($then:tt)* } else { $($els:tt)* } $(, $($rest:tt)*)?) => {
        if let $pat = $expr {
            $crate::__view_children!($builder, $($then)*);
        } else {
            $crate::__view_children!($builder, $($els)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If-let conditional without else and parenthesized expr: if let pat = (expr) { ... }
    ($builder:ident, if let $pat:pat = ( $($expr:tt)+ ) { $($then:tt)* } $(, $($rest:tt)*)?) => {
        if let $pat = $($expr)+ {
            $crate::__view_children!($builder, $($then)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If-let conditional without else and ident: if let pat = expr { ... }
    ($builder:ident, if let $pat:pat = $expr:ident { $($then:tt)* } $(, $($rest:tt)*)?) => {
        if let $pat = $expr {
            $crate::__view_children!($builder, $($then)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If conditional with else and parenthesized expr: if (cond) { ... } else { ... }
    ($builder:ident, if ( $($cond:tt)+ ) { $($then:tt)* } else { $($els:tt)* } $(, $($rest:tt)*)?) => {
        if $($cond)+ {
            $crate::__view_children!($builder, $($then)*);
        } else {
            $crate::__view_children!($builder, $($els)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If conditional with else and ident: if cond { ... } else { ... }
    ($builder:ident, if $cond:ident { $($then:tt)* } else { $($els:tt)* } $(, $($rest:tt)*)?) => {
        if $cond {
            $crate::__view_children!($builder, $($then)*);
        } else {
            $crate::__view_children!($builder, $($els)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If conditional without else and parenthesized expr: if (cond) { ... }
    ($builder:ident, if ( $($cond:tt)+ ) { $($then:tt)* } $(, $($rest:tt)*)?) => {
        if $($cond)+ {
            $crate::__view_children!($builder, $($then)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // If conditional without else and ident: if cond { ... }
    ($builder:ident, if $cond:ident { $($then:tt)* } $(, $($rest:tt)*)?) => {
        if $cond {
            $crate::__view_children!($builder, $($then)*);
        }
        $($crate::__view_children!($builder, $($rest)*);)?
    };

    // Single child expression: expr, ...
    ($builder:ident, $child:expr $(, $($rest:tt)*)?) => {
        $builder = $builder.child($child);
        $($crate::__view_children!($builder, $($rest)*);)?
    };
}
