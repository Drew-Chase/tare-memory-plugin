/// A simple row type with known stack size (3 × 8 = 24 bytes on 64-bit).
#[derive(Clone, Debug)]
pub struct Row {
    pub id: u64,
    pub value: f64,
    pub flags: u64,
}

/// Allocates a Vec with capacity, then fills it — exercises `Vec::with_capacity`
/// and `Vec::push` growth.
pub fn build_rows(n: usize) -> Vec<Row> {
    let mut rows = Vec::with_capacity(n);
    for i in 0..n {
        rows.push(Row {
            id: i as u64,
            value: i as f64 * 1.1,
            flags: 0,
        });
    }
    rows
}

/// Boxes a row — exercises `Box::new`.
pub fn box_a_row(id: u64) -> Box<Row> {
    Box::new(Row {
        id,
        value: 42.0,
        flags: 1,
    })
}

/// Collects an iterator into a Vec — exercises `.collect()`.
pub fn transform(rows: &[Row]) -> Vec<Row> {
    rows.iter()
        .map(|r| Row {
            id: r.id,
            value: r.value * 2.0,
            flags: r.flags | 0x1,
        })
        .collect()
}

/// Clones a heap type — exercises `.clone()` on String.
pub fn duplicate_data(data: &[String]) -> Vec<String> {
    data.iter().map(|s| s.clone()).collect()
}

/// Uses `format!` — exercises the format macro's implicit allocation.
pub fn label(row: &Row) -> String {
    format!("Row(id={}, value={:.2})", row.id, row.value)
}
