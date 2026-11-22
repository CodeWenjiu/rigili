use gpui_kit::*;

use super::grid::{CardView, CoverChrome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardMotion {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CardSelection {
    index: Option<usize>,
    column: usize,
}

impl CardSelection {
    pub(crate) fn index(self) -> Option<usize> {
        self.index
    }

    pub(crate) fn clear(&mut self) {
        self.index = None;
        self.column = 0;
    }

    pub(crate) fn ensure(&mut self, count: usize, columns: usize, start: usize) -> bool {
        if self.index.is_some() {
            self.clamp(count, columns)
        } else {
            self.select(start, count, columns)
        }
    }

    pub(crate) fn step(
        &mut self,
        motion: CardMotion,
        count: usize,
        columns: usize,
    ) -> Option<usize> {
        let columns = columns.max(1);
        let clamped = self.clamp(count, columns);

        let index = self.index?;
        let row = index / columns;
        let column = index % columns;
        let desired = self.column.min(columns - 1);
        let target = match motion {
            CardMotion::Left => (column > 0).then(|| index - 1),
            CardMotion::Right => (column + 1 < columns && index + 1 < count).then(|| index + 1),
            CardMotion::Up => (row > 0).then(|| in_row(row - 1, count, columns, desired)),
            CardMotion::Down => {
                let next = row + 1;
                (next * columns < count).then(|| in_row(next, count, columns, desired))
            }
        };

        let Some(target) = target else {
            return clamped.then_some(index);
        };
        if matches!(motion, CardMotion::Left | CardMotion::Right) {
            self.column = target % columns;
        }
        self.index = Some(target);
        Some(target)
    }

    fn select(&mut self, index: usize, count: usize, columns: usize) -> bool {
        let Some(index) = count.checked_sub(1).map(|last| index.min(last)) else {
            let changed = self.index.is_some();
            self.clear();
            return changed;
        };
        let column = index % columns.max(1);
        if self.index == Some(index) && self.column == column {
            return false;
        }
        self.index = Some(index);
        self.column = column;
        true
    }

    fn clamp(&mut self, count: usize, columns: usize) -> bool {
        match self.index {
            Some(index) if count > 0 && index < count => false,
            Some(_) => self.select(count.saturating_sub(1), count, columns),
            None => false,
        }
    }
}

fn in_row(row: usize, count: usize, columns: usize, column: usize) -> usize {
    let start = row * columns;
    let width = (count - start).min(columns);
    start + column.min(width - 1)
}

pub(crate) struct CardCursor {
    selection: CardSelection,
    focus: FocusHandle,
    first_row: usize,
}

impl CardCursor {
    pub(crate) fn new(focus: FocusHandle) -> Self {
        Self {
            selection: CardSelection::default(),
            focus,
            first_row: 0,
        }
    }

    pub(crate) fn observe<T: 'static>(&self, window: &mut Window, cx: &mut Context<T>) {
        let focus = self.focus.clone();
        cx.on_focus(&focus, window, |_, _, cx| cx.notify())
            .detach();
        let focus = self.focus.clone();
        cx.on_blur(&focus, window, |_, _, cx| cx.notify())
            .detach();
    }

    pub(crate) fn clear(&mut self) {
        self.selection.clear();
        self.first_row = 0;
    }

    pub(crate) fn index(&self) -> Option<usize> {
        self.selection.index()
    }

    pub(crate) fn first_row(&self) -> usize {
        self.first_row
    }

    pub(crate) fn set_first_row(&mut self, row: usize) {
        self.first_row = row;
    }

    pub(crate) fn active(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub(crate) fn view(&self, window: &Window, index: usize, chrome: CoverChrome) -> CardView {
        CardView {
            chrome,
            selected: self.index() == Some(index),
            active: self.active(window),
        }
    }

    fn start(&self, columns: usize) -> usize {
        self.first_row * columns.max(1)
    }
}

pub(crate) trait CardNav: Sized + 'static {
    fn cards(&self) -> &CardCursor;

    fn cards_mut(&mut self) -> &mut CardCursor;

    fn card_count(&self) -> usize;

    fn card_columns(&self) -> usize;

    fn reveal_card(&self, index: usize);

    fn open_card(&mut self, new_tab: bool, cx: &mut Context<Self>);

    fn card_start(&self) -> usize {
        self.cards().start(self.card_columns())
    }

    fn ensure_card(&mut self, cx: &mut Context<Self>) {
        let count = self.card_count();
        let columns = self.card_columns();
        let start = self.card_start();
        let (changed, index) = {
            let cards = self.cards_mut();
            let changed = cards.selection.ensure(count, columns, start);
            (changed, cards.index())
        };
        if let Some(index) = index {
            self.reveal_card(index);
        }
        if changed {
            cx.notify();
        }
    }

    fn step_card(&mut self, motion: CardMotion, cx: &mut Context<Self>) {
        if self.card_count() == 0 {
            return;
        }
        if self.cards().index().is_none() {
            self.ensure_card(cx);
            return;
        }
        let count = self.card_count();
        let columns = self.card_columns();
        if let Some(index) = self.cards_mut().selection.step(motion, count, columns) {
            self.reveal_card(index);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CardMotion, CardSelection};

    fn selected(start: usize, count: usize, columns: usize) -> CardSelection {
        let mut selection = CardSelection::default();
        selection.ensure(count, columns, start);
        selection
    }

    #[test]
    fn selects_the_requested_item_within_bounds() {
        assert_eq!(selected(0, 0, 3).index(), None);
        assert_eq!(selected(0, 1, 3).index(), Some(0));
        assert_eq!(selected(4, 8, 3).index(), Some(4));
        assert_eq!(selected(9, 8, 3).index(), Some(7));
    }

    #[test]
    fn horizontal_moves_stop_at_the_row_edges() {
        let mut selection = selected(3, 8, 3);
        assert_eq!(selection.step(CardMotion::Left, 8, 3), None);
        assert_eq!(selection.step(CardMotion::Right, 8, 3), Some(4));
        assert_eq!(selection.step(CardMotion::Right, 8, 3), Some(5));
        assert_eq!(selection.step(CardMotion::Right, 8, 3), None);
    }

    #[test]
    fn vertical_moves_keep_the_column_and_clamp_short_rows() {
        let mut selection = selected(5, 8, 3);
        assert_eq!(selection.step(CardMotion::Down, 8, 3), Some(7));
        assert_eq!(selection.step(CardMotion::Down, 8, 3), None);
        assert_eq!(selection.step(CardMotion::Up, 8, 3), Some(5));
        assert_eq!(selection.step(CardMotion::Up, 8, 3), Some(2));
        assert_eq!(selection.step(CardMotion::Up, 8, 3), None);
    }

    #[test]
    fn shrinking_the_list_clamps_the_selection() {
        let mut selection = selected(7, 8, 3);
        assert!(selection.ensure(4, 3, 0));
        assert_eq!(selection.index(), Some(3));
        assert!(selection.ensure(0, 3, 0));
        assert_eq!(selection.index(), None);
    }

    #[test]
    fn a_cleared_selection_is_seeded_from_the_first_visible_row() {
        let mut selection = CardSelection::default();
        const START: usize = 6;
        assert!(selection.ensure(20, 3, START));
        assert_eq!(selection.index(), Some(START));
        selection.clear();
        assert!(selection.ensure(20, 3, START));
        assert_eq!(selection.index(), Some(START));
    }
}
