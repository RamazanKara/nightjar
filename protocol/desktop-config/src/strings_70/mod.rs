//! The launcher's words added for Nightjar 7.0. The tables in `strings/<code>.rs` may not grow, so
//! every new text gets its entry here instead, in the same format; [`crate::strings`] looks a key
//! up in both tables of a language, and a key lives in exactly one of them.
//! `python tools/i18n.py apply` writes new keys here.

pub(crate) mod de;
pub(crate) mod es;
pub(crate) mod fr;
pub(crate) mod it;
pub(crate) mod ja;
pub(crate) mod ko;
pub(crate) mod pl;
pub(crate) mod pt_br;
pub(crate) mod ru;
pub(crate) mod tr;
pub(crate) mod uk;
pub(crate) mod vi;
pub(crate) mod zh_hans;
pub(crate) mod zh_hant;

#[cfg(test)]
mod tests {
    use crate::strings::{translate, Lang};

    #[test]
    fn lookups_reach_the_7_0_tables() {
        assert_eq!(translate(Lang::De, "Frame limiter"), "Bildratenbegrenzer");
        assert_eq!(translate(Lang::En, "Frame limiter"), "Frame limiter");
        assert!(Lang::De.texts().any(|t| t == "Bildratenbegrenzer"), "fonts see the 7.0 text");
    }
}
