use std::rc::Rc;

use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

pub type Open = Rc<dyn Fn(&mut Window, &mut App)>;

/// The avatar of a user, clicking through to their space when `open` is set.
pub fn avatar(
    url: impl Into<ImageSource>,
    name: impl Into<SharedString>,
    size: Pixels,
    key: u64,
    open: Option<Open>,
) -> AnyElement {
    let name = name.into();
    let avatar = div()
        .id(ElementId::NamedInteger("user-avatar".into(), key))
        .child(Avatar::new().src(url).name(name).size(size));
    match open {
        Some(open) => avatar
            .cursor_pointer()
            .on_click(move |_, window, cx| open(window, cx))
            .into_any_element(),
        None => avatar.into_any_element(),
    }
}

/// A name with the level badge users expect beside it.
pub struct Identity {
    key: u64,
    name: SharedString,
    name_size: Pixels,
    name_color: Option<Hsla>,
    level: Option<u32>,
    detail: Option<AnyElement>,
    open: Option<Open>,
}

impl Identity {
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            key: 0,
            name: name.into(),
            name_size: px(14.),
            name_color: None,
            level: None,
            detail: None,
            open: None,
        }
    }

    pub fn key(mut self, key: u64) -> Self {
        self.key = key;
        self
    }

    pub fn name_size(mut self, size: Pixels) -> Self {
        self.name_size = size;
        self
    }

    pub fn name_color(mut self, color: Hsla) -> Self {
        self.name_color = Some(color);
        self
    }

    pub fn level(mut self, level: u32) -> Self {
        self.level = Some(level);
        self
    }

    /// Rendered right after the level, such as a publish time.
    pub fn detail(mut self, detail: impl IntoElement) -> Self {
        self.detail = Some(detail.into_any_element());
        self
    }

    pub fn open(mut self, open: Option<Open>) -> Self {
        self.open = open;
        self
    }

    pub fn render(self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let mut name = div()
            .id(ElementId::NamedInteger("user-name".into(), self.key))
            .min_w_0()
            .truncate()
            .text_size(self.name_size)
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.name_color.unwrap_or(theme.foreground))
            .child(self.name);
        if let Some(open) = self.open {
            name = name
                .cursor_pointer()
                .hover(move |this| this.text_color(theme.link_hover))
                .on_click(move |_, window, cx| open(window, cx));
        }

        h_flex()
            .items_center()
            .gap_2()
            .child(name)
            .when_some(self.level.filter(|level| *level > 0), |this, level| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .px_1()
                        .rounded(px(3.))
                        .bg(theme.primary.opacity(0.12))
                        .text_color(theme.primary)
                        .text_xs()
                        .child(format!("LV{level}")),
                )
            })
            .children(self.detail)
            .into_any_element()
    }
}

/// One row crediting a user: avatar, name with its level, and whatever the
/// caller needs beside or below them.
pub struct UserRow {
    avatar: SharedString,
    identity: Identity,
    avatar_size: Pixels,
    meta: Option<AnyElement>,
    trailing: Option<AnyElement>,
    children: Vec<AnyElement>,
    start: bool,
    grow: bool,
}

impl UserRow {
    pub fn new(avatar: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        Self {
            avatar: avatar.into(),
            identity: Identity::new(name),
            avatar_size: px(40.),
            meta: None,
            trailing: None,
            children: Vec::new(),
            start: false,
            grow: false,
        }
    }

    pub fn key(mut self, key: u64) -> Self {
        self.identity = self.identity.key(key);
        self
    }

    pub fn avatar_size(mut self, size: Pixels) -> Self {
        self.avatar_size = size;
        self
    }

    pub fn name_size(mut self, size: Pixels) -> Self {
        self.identity = self.identity.name_size(size);
        self
    }

    pub fn name_color(mut self, color: Hsla) -> Self {
        self.identity = self.identity.name_color(color);
        self
    }

    pub fn level(mut self, level: u32) -> Self {
        self.identity = self.identity.level(level);
        self
    }

    pub fn detail(mut self, detail: impl IntoElement) -> Self {
        self.identity = self.identity.detail(detail);
        self
    }

    /// Rendered under the name, such as follower counts.
    pub fn meta(mut self, meta: impl IntoElement) -> Self {
        self.meta = Some(meta.into_any_element());
        self
    }

    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// Rendered below the name inside the same column, such as a comment body.
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Aligns the avatar with the first line of a taller body.
    pub fn align_start(mut self) -> Self {
        self.start = true;
        self
    }

    /// Takes the remaining width when the row sits in a horizontal flex.
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Makes the avatar and the name open the user's space.
    pub fn open(mut self, open: Option<Open>) -> Self {
        self.identity = self.identity.open(open);
        self
    }

    pub fn render(self, cx: &App) -> AnyElement {
        let open = self.identity.open.clone();
        let avatar = avatar(
            self.avatar,
            self.identity.name.clone(),
            self.avatar_size,
            self.identity.key,
            open,
        );

        let body = v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(self.identity.render(cx))
            .children(self.meta)
            .children(self.children);

        h_flex()
            .items_center()
            .when(self.start, |this| this.items_start())
            .when(self.grow, |this| this.flex_1())
            .gap_3()
            .child(avatar)
            .child(body)
            .children(self.trailing)
            .into_any_element()
    }
}
