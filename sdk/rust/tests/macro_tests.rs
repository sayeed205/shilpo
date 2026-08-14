use shilpo_ext_sdk::prelude::*;

#[test]
fn test_view_macro_and_builder_equivalence() {
    let direct = column()
        .child(text("Title").bold(true))
        .child(
            row()
                .child(icon("bell").size(16.0))
                .child(button("Click", "btn-click")),
        )
        .child(divider())
        .build();

    let from_macro = view! {
        column {
            text("Title").bold(true),
            row {
                icon("bell").size(16.0),
                button("Click", "btn-click"),
            },
            divider(),
        }
    };

    assert_eq!(direct, from_macro);
}

#[test]
fn test_view_macro_with_arguments() {
    let direct = grid(2)
        .child(icon("settings"))
        .child(text("Settings"))
        .build();

    let from_macro = view! {
        grid(2) {
            icon("settings"),
            text("Settings"),
        }
    };

    assert_eq!(from_macro.nodes.len(), 3);
    assert_eq!(from_macro, direct);
}

#[test]
fn test_view_macro_conditionals_and_loops() {
    let show_active = true;
    let show_hidden = false;
    let opt_msg = Some("Welcome");
    let items = vec!["Alpha", "Beta", "Gamma"];

    let tree = view! {
        column {
            if show_active {
                badge("Active"),
            },
            if show_hidden {
                badge("Hidden"),
            } else {
                badge("Visible"),
            },
            if let Some(msg) = opt_msg {
                text(msg),
            },
            for item in (items) {
                text(item),
            },
            divider(),
        }
    };

    assert_eq!(tree.root, 0);
    // Root has: Active badge (1), Visible badge (2), "Welcome" text (3), 3 items (4, 5, 6), divider (7)
    assert_eq!(tree.nodes.len(), 8);
    if let ViewNode::Container(c) = &tree.nodes[0] {
        assert_eq!(c.children, vec![1, 2, 3, 4, 5, 6, 7]);
    } else {
        panic!("expected container root");
    }
}
