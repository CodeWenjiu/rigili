use gpui_kit::*;

pub(super) struct FocusNav {
    handle: FocusHandle,
    subs: Option<(Subscription, Subscription)>,
    pending: bool,
}

impl FocusNav {
    pub(super) fn new<T: 'static>(cx: &mut Context<T>) -> Self {
        Self {
            handle: cx.focus_handle(),
            subs: None,
            pending: false,
        }
    }

    pub(super) fn handle(&self) -> &FocusHandle {
        &self.handle
    }

    pub(super) fn focused(&self, window: &Window) -> bool {
        self.handle.is_focused(window)
    }

    pub(super) fn request_focus(&mut self) {
        self.pending = true;
    }

    pub(super) fn observe<T: 'static>(&mut self, window: &mut Window, cx: &mut Context<T>) {
        if self.subs.is_some() {
            return;
        }
        let focus = self.handle.clone();
        let on_focus = cx.on_focus(&focus, window, |_, _, cx| cx.notify());
        let focus = self.handle.clone();
        let on_blur = cx.on_blur(&focus, window, |_, _, cx| cx.notify());
        self.subs = Some((on_focus, on_blur));
    }

    pub(super) fn apply_pending<T: 'static>(&mut self, window: &mut Window, cx: &mut Context<T>) {
        if !self.pending {
            return;
        }
        self.pending = false;
        let focus = self.handle.clone();
        window.defer(cx, move |window, cx| focus.focus(window, cx));
    }
}

pub(super) fn stepped_selection(current: Option<usize>, delta: isize, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    let Some(current) = current else {
        return Some(0);
    };
    Some((current as isize + delta).clamp(0, last as isize) as usize)
}

pub(super) fn clamp_selection(selection: Option<usize>, len: usize) -> Option<usize> {
    selection
        .filter(|_| len > 0)
        .map(|index| index.min(len - 1))
}

#[cfg(test)]
mod tests {
    use super::{clamp_selection, stepped_selection};

    #[test]
    fn steps_a_selection_within_bounds() {
        assert_eq!(stepped_selection(None, 1, 0), None);
        assert_eq!(stepped_selection(None, -1, 3), Some(0));
        assert_eq!(stepped_selection(Some(1), 1, 3), Some(2));
        assert_eq!(stepped_selection(Some(1), -1, 3), Some(0));
        assert_eq!(stepped_selection(Some(0), -1, 3), Some(0));
        assert_eq!(stepped_selection(Some(2), 1, 3), Some(2));
    }

    #[test]
    fn clamps_a_selection_to_the_list() {
        assert_eq!(clamp_selection(None, 0), None);
        assert_eq!(clamp_selection(Some(2), 0), None);
        assert_eq!(clamp_selection(Some(2), 3), Some(2));
        assert_eq!(clamp_selection(Some(7), 3), Some(2));
    }
}
