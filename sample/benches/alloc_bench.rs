use divan::Bencher;
use sample::{box_a_row, build_rows, duplicate_data, label, transform, Row};

fn main() {
    divan::main();
}

#[divan::bench(args = [100, 1000, 10000])]
fn bench_build_rows(bencher: Bencher, n: usize) {
    bencher.bench(|| build_rows(n));
}

#[divan::bench]
fn bench_box_a_row(bencher: Bencher) {
    bencher.bench(|| box_a_row(42));
}

#[divan::bench(args = [100, 1000])]
fn bench_transform(bencher: Bencher, n: usize) {
    let rows = build_rows(n);
    bencher.bench(|| transform(&rows));
}

#[divan::bench(args = [10, 100])]
fn bench_duplicate_data(bencher: Bencher, n: usize) {
    let data: Vec<String> = (0..n).map(|i| format!("item_{i}")).collect();
    bencher.bench(|| duplicate_data(&data));
}

#[divan::bench]
fn bench_label(bencher: Bencher) {
    let row = Row {
        id: 1,
        value: 3.14,
        flags: 0,
    };
    bencher.bench(|| label(&row));
}
