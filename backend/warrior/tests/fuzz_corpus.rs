//! Fuzzing の corpus の例示テスト。
//!
//! 対応する要求: なし (ADR-024 が定める Fuzzing の corpus を確かめるテストであり、
//! PRD の要求 ID を持たない)。
//! リポジトリに置いた corpus を、Fuzzing の対象のパーサにそのまま渡す。
//! どの入力でもパニックしないことを確かめる。値の正しさと往復の性質は PBT が担う。

use std::fs;
use std::path::{Path, PathBuf};

use warrior_fuzz::{run, Target};

/// リポジトリに置いた corpus のディレクトリ。
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fuzz/corpus")
}

#[test]
fn every_fuzz_target_processes_its_corpus_without_panicking() {
    for target in Target::ALL {
        let dir = corpus_dir().join(target.name());
        let mut paths = corpus_files(&dir);
        paths.sort();
        assert!(
            !paths.is_empty(),
            "the corpus of {} must contain at least one input",
            target.name()
        );
        for path in paths {
            let data = fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            run(*target, &data);
        }
    }
}

/// ディレクトリの直下のファイルを集める。ディレクトリは corpus に置かない。
fn corpus_files(dir: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read the corpus {}: {error}", dir.display()));
    entries
        .map(|entry| entry.expect("a corpus entry").path())
        .filter(|path| path.is_file())
        .collect()
}
