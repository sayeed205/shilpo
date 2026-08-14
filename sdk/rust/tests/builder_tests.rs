use shilpo_ext_sdk::prelude::*;

#[test]
fn test_all_15_node_builders_and_fields() {
    // 1. Container (Row, Column, Stack, Grid)
    let c_row = row()
        .gap(10.0)
        .align_items(Alignment::Center)
        .justify_content(Justification::SpaceBetween)
        .wrap(true)
        .event_id("row-click")
        .style(style().padding(5.0))
        .build();
    assert_eq!(c_row.root, 0);
    if let ViewNode::Container(c) = &c_row.nodes[0] {
        assert_eq!(c.direction, ContainerDirection::Row);
        assert_eq!(c.gap, Some(10.0));
        assert_eq!(c.align_items, Some(Alignment::Center));
        assert_eq!(c.justify_content, Some(Justification::SpaceBetween));
        assert!(c.wrap);
        assert_eq!(c.event_id.as_deref(), Some("row-click"));
        assert_eq!(c.style.as_ref().unwrap().padding, Some(5.0));
    } else {
        panic!("expected container node");
    }

    let c_col = column().direction(ContainerDirection::Column).build();
    if let ViewNode::Container(c) = &c_col.nodes[0] {
        assert_eq!(c.direction, ContainerDirection::Column);
    }

    let c_stack = stack().build();
    if let ViewNode::Container(c) = &c_stack.nodes[0] {
        assert_eq!(c.direction, ContainerDirection::Stack);
    }

    let c_grid = grid(3).build();
    if let ViewNode::Container(c) = &c_grid.nodes[0] {
        assert_eq!(c.direction, ContainerDirection::Grid(3));
    }

    // 2. Text
    let t = text("Headline")
        .font_size(18.0)
        .bold(true)
        .style(style().color(SemanticColorToken::Primary))
        .build();
    if let ViewNode::Text(tn) = &t.nodes[0] {
        assert_eq!(tn.content, "Headline");
        assert_eq!(tn.font_size, Some(18.0));
        assert_eq!(tn.bold, Some(true));
        assert_eq!(
            tn.style.as_ref().unwrap().color,
            Some(SemanticColorToken::Primary)
        );
    } else {
        panic!("expected text node");
    }

    // 3. Icon
    let ic = icon("star")
        .size(24.0)
        .style(style().color(SemanticColorToken::Secondary))
        .build();
    if let ViewNode::Icon(in_) = &ic.nodes[0] {
        assert_eq!(in_.name, "star");
        assert_eq!(in_.size, Some(24.0));
    } else {
        panic!("expected icon node");
    }

    // 4. Image
    let img = image("assets/logo.png")
        .width(100.0)
        .height(50.0)
        .style(style().opacity(0.8))
        .build();
    if let ViewNode::Image(im) = &img.nodes[0] {
        assert_eq!(im.asset_path, "assets/logo.png");
        assert_eq!(im.width, Some(100.0));
        assert_eq!(im.height, Some(50.0));
        assert_eq!(im.style.as_ref().unwrap().opacity, Some(0.8));
    } else {
        panic!("expected image node");
    }

    // 5. Button
    let btn = button("Submit", "submit-event")
        .style(style().background(SemanticColorToken::Primary))
        .build();
    if let ViewNode::Button(bn) = &btn.nodes[0] {
        assert_eq!(bn.label, "Submit");
        assert_eq!(bn.event_id, "submit-event");
    } else {
        panic!("expected button node");
    }

    // 6. IconButton
    let ibtn = icon_button("close", "close-event").build();
    if let ViewNode::IconButton(ib) = &ibtn.nodes[0] {
        assert_eq!(ib.icon_name, "close");
        assert_eq!(ib.event_id, "close-event");
    } else {
        panic!("expected icon button node");
    }

    // 7. Toggle
    let tog = toggle(true, "toggle-event").build();
    if let ViewNode::Toggle(tg) = &tog.nodes[0] {
        assert!(tg.value);
        assert_eq!(tg.event_id, "toggle-event");
    } else {
        panic!("expected toggle node");
    }

    // 8. Slider
    let sld = slider(50.0, 0.0, 100.0, "slider-event").build();
    if let ViewNode::Slider(sl) = &sld.nodes[0] {
        assert_eq!(sl.value, 50.0);
        assert_eq!(sl.min, 0.0);
        assert_eq!(sl.max, 100.0);
        assert_eq!(sl.event_id, "slider-event");
    } else {
        panic!("expected slider node");
    }

    // 9. TextInput
    let inp = text_input("Initial text", "input-event")
        .placeholder("Type here...")
        .build();
    if let ViewNode::TextInput(ti) = &inp.nodes[0] {
        assert_eq!(ti.value, "Initial text");
        assert_eq!(ti.placeholder.as_deref(), Some("Type here..."));
        assert_eq!(ti.event_id, "input-event");
    } else {
        panic!("expected text input node");
    }

    // 10. List
    let lst = list().item(text("Item 1")).item(text("Item 2")).build();
    assert_eq!(lst.nodes.len(), 3);
    if let ViewNode::List(ln) = &lst.nodes[0] {
        assert_eq!(ln.items, vec![1, 2]);
    } else {
        panic!("expected list node");
    }

    // 11. Spacer
    let spc = spacer().size(16.0).build();
    if let ViewNode::Spacer(sp) = &spc.nodes[0] {
        assert_eq!(sp.size, Some(16.0));
    } else {
        panic!("expected spacer node");
    }

    // 12. Divider
    let div = build_view_tree(divider());
    assert_eq!(div.nodes[0], ViewNode::Divider);

    // 13. Badge
    let bdg = badge("New").build();
    if let ViewNode::Badge(bg) = &bdg.nodes[0] {
        assert_eq!(bg.label, "New");
    } else {
        panic!("expected badge node");
    }

    // 14. Progress
    let prg = progress(0.75).build();
    if let ViewNode::Progress(p) = &prg.nodes[0] {
        assert_eq!(p.value, 0.75);
    } else {
        panic!("expected progress node");
    }

    // 15. LoadingIndicator
    let ldi = loading_indicator()
        .size(32.0)
        .color(SemanticColorToken::Primary)
        .build();
    if let ViewNode::LoadingIndicator(li) = &ldi.nodes[0] {
        assert_eq!(li.size, Some(32.0));
        assert_eq!(li.color, Some(SemanticColorToken::Primary));
    } else {
        panic!("expected loading indicator node");
    }
}

#[test]
fn test_style_builder_all_fields() {
    let s = style()
        .padding(8.0)
        .margin(4.0)
        .width(200.0)
        .height(100.0)
        .corner_radius(12.0)
        .opacity(0.9)
        .color(SemanticColorToken::OnSurface)
        .background(SemanticColorToken::SurfaceContainer)
        .flex_grow(1.0)
        .border_width(2.0)
        .border_color(SemanticColorToken::Outline)
        .min_width(50.0)
        .max_width(300.0)
        .min_height(25.0)
        .max_height(150.0)
        .overflow(Overflow::Scroll)
        .build();

    assert_eq!(s.padding, Some(8.0));
    assert_eq!(s.margin, Some(4.0));
    assert_eq!(s.width, Some(200.0));
    assert_eq!(s.height, Some(100.0));
    assert_eq!(s.corner_radius, Some(12.0));
    assert_eq!(s.opacity, Some(0.9));
    assert_eq!(s.color, Some(SemanticColorToken::OnSurface));
    assert_eq!(s.background, Some(SemanticColorToken::SurfaceContainer));
    assert_eq!(s.flex_grow, Some(1.0));
    assert_eq!(s.border_width, Some(2.0));
    assert_eq!(s.border_color, Some(SemanticColorToken::Outline));
    assert_eq!(s.min_width, Some(50.0));
    assert_eq!(s.max_width, Some(300.0));
    assert_eq!(s.min_height, Some(25.0));
    assert_eq!(s.max_height, Some(150.0));
    assert_eq!(s.overflow, Some(Overflow::Scroll));
}

#[test]
fn test_deterministic_flattening_and_child_ordering() {
    let tree = column()
        .child(text("Header"))
        .child(
            row()
                .child(icon("bell"))
                .child(text("Notification"))
                .child(button("Dismiss", "dismiss")),
        )
        .child(divider())
        .child(list().item(text("Item A")).item(text("Item B")))
        .build();

    assert_eq!(tree.root, 0);
    assert_eq!(tree.nodes.len(), 10);

    // Node 0: Root Column
    if let ViewNode::Container(c) = &tree.nodes[0] {
        assert_eq!(c.direction, ContainerDirection::Column);
        // Children of root: Header (1), Row (2), Divider (6), List (7)
        assert_eq!(c.children, vec![1, 2, 6, 7]);
    } else {
        panic!("expected root container");
    }

    // Node 1: Header text
    assert!(matches!(&tree.nodes[1], ViewNode::Text(t) if t.content == "Header"));

    // Node 2: Row container
    if let ViewNode::Container(c) = &tree.nodes[2] {
        assert_eq!(c.direction, ContainerDirection::Row);
        // Children of row: icon (3), text (4), button (5)
        assert_eq!(c.children, vec![3, 4, 5]);
    } else {
        panic!("expected row container");
    }

    // Node 3, 4, 5
    assert!(matches!(&tree.nodes[3], ViewNode::Icon(i) if i.name == "bell"));
    assert!(matches!(&tree.nodes[4], ViewNode::Text(t) if t.content == "Notification"));
    assert!(matches!(&tree.nodes[5], ViewNode::Button(b) if b.label == "Dismiss"));

    // Node 6: Divider
    assert_eq!(tree.nodes[6], ViewNode::Divider);

    // Node 7: List container
    if let ViewNode::List(l) = &tree.nodes[7] {
        assert_eq!(l.items, vec![8, 9]);
    } else {
        panic!("expected list container");
    }

    // Node 8, 9
    assert!(matches!(&tree.nodes[8], ViewNode::Text(t) if t.content == "Item A"));
    assert!(matches!(&tree.nodes[9], ViewNode::Text(t) if t.content == "Item B"));
}

#[test]
fn test_optional_and_iterated_children() {
    let maybe_badge: Option<BadgeBuilder> = Some(badge("Pro"));
    let no_badge: Option<BadgeBuilder> = None;
    let items = vec!["One", "Two", "Three"];

    let tree = column()
        .child_opt(maybe_badge)
        .child_opt(no_badge)
        .children(items.into_iter().map(text))
        .build();

    assert_eq!(tree.root, 0);
    if let ViewNode::Container(c) = &tree.nodes[0] {
        assert_eq!(c.children.len(), 4); // 1 badge + 3 text items
        assert_eq!(c.children, vec![1, 2, 3, 4]);
    }
}
