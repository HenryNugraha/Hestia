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

    pub(super) fn typing(&self) -> bool {
        self.typing
    }

    pub(super) fn active(&self) -> bool {
        self.typing || !self.query.is_empty()
    }

    pub(super) fn toggle(&mut self) {
        self.typing = !self.typing;
    }

    pub(super) fn start_typing(&mut self) {
        self.typing = true;
    }

    pub(super) fn stop_typing(&mut self) {
        self.typing = false;
    }

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

        search.query_mut().push_str("vow");
        assert!(search.escape());
        assert!(!search.escape());

        search.start_typing();
        assert!(search.escape());
        assert!(!search.typing());
    }
}
