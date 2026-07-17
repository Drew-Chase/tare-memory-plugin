#[cfg(feature = "tare-profile")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use sample::{box_a_row, build_rows, duplicate_data, label, transform};

fn main() {
    #[cfg(feature = "tare-profile")]
    let _profiler = dhat::Profiler::new_heap();

    // Exercise all known allocation patterns.
    let rows = build_rows(1000);
    let _boxed = box_a_row(99);
    let transformed = transform(&rows);
    let labels: Vec<String> = rows.iter().map(|r| label(r)).collect();
    let _cloned = duplicate_data(&labels);

    println!(
        "Done: {} rows, {} transformed, {} labels",
        rows.len(),
        transformed.len(),
        labels.len()
    );
}
