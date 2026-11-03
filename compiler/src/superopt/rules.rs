pub(crate) struct Rule {
    pub(crate) pattern: &'static [&'static str],
    pub(crate) replacement: &'static [&'static str],
    pub(crate) dead: u32,
    pub(crate) flags_dead: bool,
    pub(crate) hits: u32,
}

pub(crate) const RULES: &[Rule] = &[
    Rule { pattern: &["mov %0, %0"], replacement: &[], dead: 0, flags_dead: false, hits: 129 },
    Rule { pattern: &["mov %0, %1", "mov %2, %2"], replacement: &["mov %0, %1"], dead: 0, flags_dead: false, hits: 50 },
];
