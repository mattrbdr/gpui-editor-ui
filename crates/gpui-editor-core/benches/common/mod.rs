//! Generated documents shared by the benchmarks and the timing tests.

use std::fmt::Write as _;

/// Text blocks produced by one section of [`markdown`].
pub const BLOCKS_PER_SECTION: usize = 9;

/// A Markdown document of about `blocks` text blocks: headings, marks, links, nested lists, tasks
/// and quotes, repeated by sections of [`BLOCKS_PER_SECTION`] blocks.
pub fn markdown(blocks: usize) -> String {
    let mut out = String::new();
    for section in 0..blocks.div_ceil(BLOCKS_PER_SECTION) {
        writeln!(out, "## Section {section}\n").unwrap();
        out.push_str(
            "Some **bold** text, some *italic* text and a [link](https://example.com).\n\n",
        );
        out.push_str("- First item\n- Second item\n  - Nested item\n\n");
        out.push_str("- [ ] A task\n- [x] Done\n\n");
        out.push_str("> A quote\n\n");
        out.push_str("Closing paragraph of the section.\n\n");
    }
    out
}
