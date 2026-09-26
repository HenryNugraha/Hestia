//! Search state for the native overlay preview.
//!
//! Tab, F or Ctrl+F start typing.  Tab stops typing and keeps the results, so
//! the normal keys work on them.  Esc clears the search, and Esc again closes
//! the overlay.

#[derive(Debug, Default)]
pub(super) struct Search {
    query: String,
    typing: bool,
}

impl Search {
    pub(super) fn query(&self) -> &str {
        &self.query
    }

    pub(super) fn query_mut(&mut self) -> &mut String {
        &mut self.query
    }

    /// Whether letters and Space type into the search instead of moving.
    pub(super) fn typing(&self) -> bool {
        self.typing
    }

    /// Whether the search field shows: while typing, or while it filters.
    pub(super) fn active(&self) -> bool {
        self.typing || !self.query.is_empty()
    }

    /// Tab, F or Ctrl+F.  F types while searching, so the keyboard only sends
    /// this for it when typing is off.
    pub(super) fn toggle(&mut self) {
        self.typing = !self.typing;
    }

    pub(super) fn start_typing(&mut self) {
        self.typing = true;
    }

    pub(super) fn stop_typing(&mut self) {
        self.typing = false;
    }

    /// Clears an active search and returns true.  Returns false when there
    /// was nothing to clear, so Esc can close the overlay instead.
    pub(super) fn escape(&mut self) -> bool {
        let active = self.active();
        self.clear();
        active
    }

    pub(super) fn clear(&mut self) {
        self.query.clear();
        self.typing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_starts_and_stops_typing_and_keeps_the_results() {
        let mut search = Search::default();
        assert!(!search.active());
        search.toggle();
        assert!(search.typing() && search.active());
        search.query_mut().push_str("vow");
        search.toggle();
        assert!(!search.typing());
        assert!(search.active());
        assert_eq!(search.query(), "vow");
        // Tab again goes back to editing the same search.
        search.toggle();
        assert!(search.typing());
        assert_eq!(search.query(), "vow");
    }

    #[test]
    fn an_empty_search_hides_once_typing_stops() {
        let mut search = Search::default();
        search.start_typing();
        assert!(search.active());
        search.stop_typing();
        assert!(!search.active());
    }

    #[test]
    fn escape_clears_first_and_then_lets_the_overlay_close() {
        let mut search = Search::default();
        search.start_typing();
        search.query_mut().push_str("vow");
        assert!(search.escape());
        assert!(!search.active());
        assert_eq!(search.query(), "");
        assert!(!search.escape());

        // Results that are no longer being typed clear the same way.
        search.query_mut().push_str("vow");
        assert!(search.escape());
        assert!(!search.escape());

        // So does an empty search that is being typed.
        search.start_typing();
        assert!(search.escape());
        assert!(!search.typing());
    }
}
