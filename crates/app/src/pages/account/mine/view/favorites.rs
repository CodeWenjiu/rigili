use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::FavFolder;

use super::{
    FavoriteView, FolderSource, MineTab, MineView, create_folder::CreateFolderDialog,
    sort_folders::SortFoldersDialog,
};
use crate::shared::browse::paging::Paged;
use crate::shared::ui::tabs::SubTabNav;

impl MineView {
    fn current_folder(&self) -> Option<&FavFolder> {
        let id = self.folders.current?;
        self.folders
            .folders
            .iter()
            .chain(self.collected.folders.iter())
            .find(|folder| folder.id == id)
    }

    fn show_favorite_view(&mut self, view: FavoriteView, cx: &mut Context<Self>) {
        if self.favorites_view == view {
            if self.browsing_folder.is_some() {
                self.browsing_folder = None;
                cx.notify();
            }
            return;
        }
        self.favorites_view = view;
        self.browsing_folder = None;
        if view == FavoriteView::Folder {
            self.ensure_default_folder(cx);
            if self.favorites_keyword != self.search.keyword() {
                self.load_favorite_items(cx);
            }
        } else {
            self.load_favorite_view(cx);
        }
        cx.notify();
    }

    pub(super) fn default_folder_id(&self) -> Option<u64> {
        self.folders
            .folders
            .iter()
            .find(|folder| folder.is_default())
            .map(|folder| folder.id)
            .or_else(|| self.folders.folders.first().map(|folder| folder.id))
    }

    fn ensure_default_folder(&mut self, cx: &mut Context<Self>) {
        let default_id = self.default_folder_id();
        if self.folders.current == default_id {
            return;
        }
        self.folders.current = default_id;
        if default_id.is_some() {
            self.load_favorite_items(cx);
        } else {
            self.favorites = Paged::new();
        }
    }

    fn close_folder(&mut self, cx: &mut Context<Self>) {
        self.browsing_folder = None;
        cx.notify();
    }

    pub(super) fn load_favorite_view(&mut self, cx: &mut Context<Self>) {
        if self.browsing_folder.is_some() && self.favorites_keyword != self.search.keyword() {
            self.load_favorite_items(cx);
            return;
        }
        match self.favorites_view {
            FavoriteView::Folder if self.favorites_keyword != self.search.keyword() => {
                self.load_favorite_items(cx);
            }
            FavoriteView::Subscribed
                if !self.collected.loaded
                    && !self.collected.loading
                    && self.collected.error.is_none() =>
            {
                self.load_collected(cx);
            }
            FavoriteView::Bangumi if self.bangumi.needs_first_load() => {
                self.load_bangumi(false, cx);
            }
            FavoriteView::Cinema if self.cinema.needs_first_load() => {
                self.load_bangumi(true, cx);
            }
            _ => {}
        }
    }

    fn render_favorite_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let hover_bg = cx.theme().accent.opacity(0.5);
        let radius = cx.theme().radius;

        h_flex()
            .flex_shrink_0()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(FavoriteView::ALL.map(|view| {
                let active = view == self.favorites_view;
                let group = SharedString::from(format!("mine-favorite-view-{}", view.index()));
                let label: String = match view {
                    FavoriteView::Folder => "默认收藏夹".into(),
                    FavoriteView::Folders => "我创建的收藏夹".into(),
                    FavoriteView::Subscribed => "我的收藏与订阅".into(),
                    FavoriteView::Bangumi => "我的追番".into(),
                    FavoriteView::Cinema => "我的追剧".into(),
                };

                v_flex()
                    .group(group.clone())
                    .items_center()
                    .rounded(radius)
                    .when(!active, |this| this.hover(move |this| this.bg(hover_bg)))
                    .child(
                        Button::new(("mine-favorite-view", view.index()))
                            .text()
                            .small()
                            .px_2()
                            .accessibility_label(label.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_sub_tab(view, cx);
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if active { primary } else { muted })
                                    .group_hover(group, move |this| this.text_color(primary))
                                    .child(label),
                            ),
                    )
            }))
            .when(self.favorites_view == FavoriteView::Folders, |this| {
                this.child(div().flex_1()).child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_2()
                        .child(
                            Button::new("mine-create-folder")
                                .small()
                                .secondary()
                                .label("+ 新建收藏夹")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_create_folder(window, cx);
                                })),
                        )
                        .when(
                            self.folders
                                .folders
                                .iter()
                                .any(|folder| !folder.is_default()),
                            |this| {
                                this.child(
                                    Button::new("mine-sort-folders")
                                        .small()
                                        .ghost()
                                        .label("收藏夹排序")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_sort_folders(window, cx);
                                        })),
                                )
                            },
                        ),
                )
            })
            .into_any_element()
    }

    fn open_create_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mine = cx.entity();
        let view = cx.new(|cx| CreateFolderDialog::new(window, cx, mine));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(320.)).title("新建收藏夹").child(view.clone())
        });
    }

    fn open_sort_folders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folders = self
            .folders
            .folders
            .iter()
            .filter(|folder| !folder.is_default())
            .cloned()
            .collect::<Vec<_>>();
        if folders.is_empty() {
            return;
        }
        let default_id = self
            .folders
            .folders
            .iter()
            .find(|folder| folder.is_default())
            .map(|folder| folder.id);
        let mine = cx.entity();
        let view = cx.new(|_| SortFoldersDialog::new(window, mine, folders, default_id));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(360.)).title("收藏夹排序").child(view.clone())
        });
    }

    pub(super) fn folders_changed(&mut self, cx: &mut Context<Self>) {
        self.favorites_view = FavoriteView::Folders;
        self.browsing_folder = None;
        self.load_folders(cx);
        cx.notify();
    }

    fn render_folder_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let title = self
            .current_folder()
            .map(|folder| folder.title.clone())
            .unwrap_or_default();

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("mine-folder-back")
                            .text()
                            .small()
                            .text_color(muted)
                            .label("← 返回")
                            .on_click(cx.listener(|this, _, _, cx| this.close_folder(cx))),
                    )
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title)),
            )
            .child(self.render_content(MineTab::Favorites, window, cx))
            .into_any_element()
    }

    pub(super) fn render_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;

        if self.tab == MineTab::Offline {
            return div()
                .flex_1()
                .min_h_0()
                .text_sm()
                .text_color(muted)
                .child("离线缓存暂未支持")
                .into_any_element();
        }

        let content = match self.tab {
            MineTab::Favorites => match self.favorites_view {
                FavoriteView::Folder => self.render_content(MineTab::Favorites, window, cx),
                FavoriteView::Folders => {
                    if self.browsing_folder == Some(FavoriteView::Folders) {
                        self.render_folder_detail(window, cx)
                    } else {
                        self.render_folder_grid(FolderSource::Created, window, cx)
                    }
                }
                FavoriteView::Subscribed => {
                    if self.browsing_folder == Some(FavoriteView::Subscribed) {
                        self.render_folder_detail(window, cx)
                    } else {
                        self.render_folder_grid(FolderSource::Collected, window, cx)
                    }
                }
                FavoriteView::Bangumi => self.render_bangumi(false, window, cx),
                FavoriteView::Cinema => self.render_bangumi(true, window, cx),
            },
            _ => self.render_content(self.tab, window, cx),
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .when(self.tab == MineTab::Favorites, |this| {
                this.child(self.render_favorite_tabs(cx))
            })
            .child(content)
            .into_any_element()
    }
}

impl SubTabNav for MineView {
    type SubTab = FavoriteView;

    const SUB_TABS: &'static [FavoriteView] = &FavoriteView::ALL;

    fn active_sub_tab(&self) -> Option<FavoriteView> {
        (self.tab == MineTab::Favorites).then_some(self.favorites_view)
    }

    fn select_sub_tab(&mut self, view: FavoriteView, cx: &mut Context<Self>) {
        self.show_favorite_view(view, cx);
    }
}
