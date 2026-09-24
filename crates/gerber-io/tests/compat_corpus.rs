//! Local real-file compatibility inventory. Run explicitly with RCAM_ROOT and
//! RCAM_CANDIDATE_PATHS; never uploads or changes a source file.

#[test]
#[ignore = "requires local Gerber corpus and RCAM_* paths"]
fn scan_real_corpus() {
    let root = std::env::var("RCAM_ROOT").expect("RCAM_ROOT");
    let list = std::env::var("RCAM_CANDIDATE_PATHS").expect("RCAM_CANDIDATE_PATHS");
    let paths = std::fs::read_to_string(list).unwrap();
    println!("status\tbytes\tobjects\twarnings\tpath\terror");
    for relative in paths.lines() {
        let path = std::path::Path::new(&root).join(relative);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("IO\t0\t0\t0\t{relative}\t{error}");
                continue;
            }
        };
        match gerber_io::parse_s1(&bytes, relative) {
            Ok(scene) => println!(
                "STRICT\t{}\t{}\t0\t{relative}\t",
                bytes.len(),
                scene.document.object_count()
            ),
            Err(strict) => match gerber_io::parse_s1_compat(&bytes, relative) {
                Ok(scene) => println!(
                    "COMPAT\t{}\t{}\t{}\t{relative}\t{strict}",
                    bytes.len(),
                    scene.document.object_count(),
                    scene.metadata.compatibility_issues.join(";")
                ),
                Err(error) => println!("REJECTED\t{}\t0\t0\t{relative}\t{error}", bytes.len()),
            },
        }
    }
}
