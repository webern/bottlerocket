use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;

/// This program serves as a pseudo-migration for testing. It will write its name and arguments to
/// a file named `results.txt` one level above the directory it receives as the `--source-datastore`
fn main() {
    // the name of the program is lost with pentacle, so we hardcode a name here.
    let mut message = "migration-name-replaceme:".to_owned();
    let mut args = std::env::args();
    let mut datastore_argument = String::new();
    if let Some(_) = args.next() {
        // i == 0 is the first arg after the binary path, i.e. shifted by one
        for (i, arg) in args.enumerate() {
            message.push(' ');
            message.push_str(&arg);
            if i == 2 {
                // this is the position where we expect to find the datastore path, e.g.
                // i ==>     0         1                  2        3                  4
                // migration --forward --source-datastore /some/ds --target-datastore /another/ds
                datastore_argument = arg.to_string();
            }
        }
    }
    let mut outdir = PathBuf::from_str(datastore_argument.as_str()).unwrap();
    // remove the datastore dir from the path leaving the parent dir.
    outdir.pop();
    // make sure we error if there's a path issue.
    outdir = outdir.canonicalize().unwrap();
    assert!(!outdir.to_string_lossy().is_empty());
    let outfile_path = outdir.join("result.txt");
    // create or append
    let mut f = if !outfile_path.is_file() {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&outfile_path)
            .unwrap()
    } else {
        std::fs::OpenOptions::new()
            .create(false)
            .append(true)
            .open(&outfile_path)
            .unwrap()
    };
    write!(f, "{}\n", message).unwrap();
}
