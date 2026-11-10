//! Reading a layout file.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::{Axis, Condition, Kind, Layout, Node, Size};

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

fn err<T>(msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(msg.into()))
}

fn size(v: &str) -> Size {
    match v.trim().parse::<f32>() {
        Ok(p) => Size::Points(p),
        Err(_) => Size::Input(v.trim().to_owned()),
    }
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A node from an element's tag and attributes (children are added later).
fn node(e: &BytesStart) -> Result<Node, ParseError> {
    let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut n = Node {
        kind: match tag.as_str() {
            "vstack" | "hstack" => Kind::Stack {
                axis: if tag == "vstack" {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                },
                spacing: Size::Points(0.0),
                children: Vec::new(),
            },
            "slot" => Kind::Slot,
            "gap" => Kind::Gap,
            "group" => Kind::Group {
                children: Vec::new(),
            },
            other => return err(format!("unknown element <{other}>")),
        },
        name: None,
        size: None,
        grow: Size::Points(0.0),
        visible: None,
    };
    for a in e.attributes() {
        let a = a.map_err(|e| ParseError(e.to_string()))?;
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let value = a
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| ParseError(e.to_string()))?
            .into_owned();
        match key.as_str() {
            "name" => {
                if !is_ident(&value) {
                    return err(format!("<{tag}> name {value:?} isn't an identifier"));
                }
                n.name = Some(value);
            }
            "height" | "width" => n.size = Some(size(&value)),
            "grow" => n.grow = size(&value),
            "spacing" => match &mut n.kind {
                Kind::Stack { spacing, .. } => *spacing = size(&value),
                _ => return err(format!("<{tag}> has no spacing")),
            },
            "visible" => {
                let (negated, flag) = match value.trim().strip_prefix('!') {
                    Some(f) => (true, f.trim()),
                    None => (false, value.trim()),
                };
                if !is_ident(flag) {
                    return err(format!("visible={value:?} isn't a flag name"));
                }
                n.visible = Some(Condition {
                    flag: flag.to_owned(),
                    negated,
                });
            }
            other => return err(format!("<{tag}> has no attribute {other:?}")),
        }
    }
    for s in [&n.size, &Some(n.grow.clone())].into_iter().flatten() {
        if let Size::Input(name) = s
            && !is_ident(name)
        {
            return err(format!("{name:?} is neither a number nor an input name"));
        }
    }
    if matches!(n.kind, Kind::Group { .. })
        && (n.name.is_some() || n.size.is_some() || n.grow != Size::Points(0.0))
    {
        return err("a <group> only wraps: it takes `visible`, no name or size");
    }
    if matches!(n.kind, Kind::Slot) && n.name.is_none() {
        return err("a <slot> needs a name");
    }
    Ok(n)
}

/// Parses a layout file: a `<layout name="...">` around one root element.
pub fn parse(xml: &str) -> Result<Layout, ParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut name = None;
    // Open elements, innermost last.
    let mut open: Vec<Node> = Vec::new();
    let mut root = None;
    let push = |open: &mut Vec<Node>, root: &mut Option<Node>, n: Node| {
        match open.last_mut() {
            Some(Node {
                kind: Kind::Stack { children, .. } | Kind::Group { children },
                ..
            }) => children.push(n),
            Some(_) => return err("only stacks have children"),
            None if root.is_none() => *root = Some(n),
            None => return err("a layout has one root element"),
        }
        Ok(())
    };
    loop {
        match reader.read_event() {
            Err(e) => return err(format!("at {}: {e}", reader.buffer_position())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) if e.name().as_ref() == b"layout" => {
                for a in e.attributes().flatten() {
                    if a.key.as_ref() == b"name" {
                        name = Some(String::from_utf8_lossy(&a.value).into_owned());
                    }
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == b"layout" => {}
            Ok(Event::Start(e)) => open.push(node(&e)?),
            Ok(Event::Empty(e)) => push(&mut open, &mut root, node(&e)?)?,
            Ok(Event::End(_)) => {
                let Some(n) = open.pop() else {
                    return err("unbalanced end tag");
                };
                push(&mut open, &mut root, n)?;
            }
            Ok(Event::Comment(_) | Event::Decl(_) | Event::Text(_)) => {}
            Ok(_) => {}
        }
    }
    let Some(name) = name.filter(|n| is_ident(n)) else {
        return err("<layout> needs a name that's an identifier");
    };
    let Some(root) = root else {
        return err("the layout is empty");
    };
    if matches!(root.kind, Kind::Group { .. }) {
        return err("a <group> needs a stack around it");
    }
    Ok(Layout { name, root })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stacks_slots_gaps_sizes_and_conditions() {
        let l = parse(
            r#"<layout name="Demo">
                 <vstack spacing="2">
                   <slot name="top" height="top_height" visible="!hidden"/>
                   <gap height="4"/>
                   <hstack name="row" grow="1"><slot name="cell" grow="1"/></hstack>
                 </vstack>
               </layout>"#,
        )
        .unwrap();
        assert_eq!(l.name, "Demo");
        let Kind::Stack {
            children, spacing, ..
        } = &l.root.kind
        else {
            panic!()
        };
        assert_eq!(*spacing, Size::Points(2.0));
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].size, Some(Size::Input("top_height".into())));
        assert_eq!(
            children[0].visible,
            Some(Condition {
                flag: "hidden".into(),
                negated: true
            })
        );
        assert_eq!(children[2].name.as_deref(), Some("row"));
    }

    #[test]
    fn groups_wrap_children_and_take_only_visible() {
        let l = parse(
            r#"<layout name="G"><vstack>
                 <group visible="on"><gap height="1"/><slot name="a" grow="weight"/></group>
               </vstack></layout>"#,
        )
        .unwrap();
        let Kind::Stack { children, .. } = &l.root.kind else {
            panic!()
        };
        let Kind::Group { children: inner } = &children[0].kind else {
            panic!()
        };
        assert_eq!(inner[1].grow, Size::Input("weight".into()));
        assert!(parse(r#"<layout name="G"><vstack><group name="x"/></vstack></layout>"#).is_err());
        assert!(parse(r#"<layout name="G"><group><slot name="a"/></group></layout>"#).is_err());
    }

    #[test]
    fn mistakes_are_reported() {
        assert!(parse(r#"<layout name="x"><box/></layout>"#).is_err());
        assert!(parse(r#"<layout name="x"><slot/></layout>"#).is_err());
        assert!(parse(r#"<layout name="x"><slot name="a" height="1+2"/></layout>"#).is_err());
        assert!(parse(r#"<layout><slot name="a"/></layout>"#).is_err());
    }
}
