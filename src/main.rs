use std::path::PathBuf;
use std::process::Command;

#[derive(Default)]
struct Gvim();

// I picked these values off the top of my head
const MAX_ARGS: usize = 20;
const MAX_FILES: usize = 30;
const MAX_SIZE: u64 = 1024 * 300;

impl Gvim {
    const PROCESS_RUNNING_TIME_THRESHOLD_IN_SECS: u64 = 3;
    const GVIM_REUSE_INSTANCE_OPTIONS: [&str; 3] = ["--server-name", "GVIM", "--remote-tab"];
    #[cfg(target_os = "windows")]
    const DETACHED_PROCESS: u32 = 0x00000008;

    fn new() -> Self {
        Gvim::default()
    }

    fn check_process(&self) -> Option<u64> {
        let mut system = sysinfo::System::new();

        system.refresh_specifics(
            sysinfo::RefreshKind::nothing()
                .with_processes(sysinfo::ProcessRefreshKind::everything()),
        );

        // Let's check if there's already gvim instance or not
        if let Some((_, p)) = system
            .processes()
            .iter()
            .find(|(_, p)| p.name() == "gvim" || p.name() == "gvim.exe")
        {
            let run_secs = p.run_time();

            Some(run_secs)
        } else {
            None
        }
    }

    fn open(&self, normalized_paths: &Vec<PathBuf>) {
        if let Some(running_time) = self.check_process() {
            // Reuse a existing gvim instance.

            // If no arguments have been supplied, there is nothing to do.
            if normalized_paths.is_empty() {
                return;
            }

            // Notice: just-launched gvim instance might have no remote functionalities yet.
            // So for such cases we need to "wait" for a moment before the following execution.
            // Not sure how long should we wait for but 3 seconds must be at most sufficient.
            let rest = Self::PROCESS_RUNNING_TIME_THRESHOLD_IN_SECS.saturating_sub(running_time);

            std::thread::sleep(std::time::Duration::from_secs(rest));

            self.exec_gvim(Self::GVIM_REUSE_INSTANCE_OPTIONS, normalized_paths);
        } else {
            // Create a new gvim instance.

            self.exec_gvim([""; 0], normalized_paths);
        }
    }

    fn exec_gvim<I, S, T, U>(&self, options: I, args: T)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
        T: IntoIterator<Item = U>,
        U: AsRef<std::ffi::OsStr>,
    {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            Command::new("gvim")
                .args(options)
                .args(args)
                .creation_flags(Self::DETACHED_PROCESS)
                .spawn();
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("gvim").args(options).args(args).spawn();
        }

        #[cfg(target_os = "linux")]
        {
            Command::new("gvim")
                .env("GDK_BACKEND", "x11")
                .args(options)
                .args(args)
                .spawn();
        }
    }
}

struct App {
    args: Vec<String>,
    gvim: Gvim,
    files: Vec<PathBuf>,
}

impl App {
    fn new() -> App {
        App {
            args: std::env::args().collect(),
            gvim: Gvim::new(),
            files: vec![],
        }
    }

    fn has_too_many_arguments(&self) -> bool {
        if self.args.len() > MAX_ARGS + 1 {
            return true;
        } else {
            return false;
        }
    }

    fn has_large_size_of_files(&self) -> bool {
        let mut sum = 0;
        let mut res = false;

        self.files.iter().for_each(|f| match std::fs::metadata(f) {
            Ok(metadata) => {
                let size = metadata.len();

                sum += size;

                if sum > MAX_SIZE {
                    res = true;
                }
            }
            Err(_) => {}
        });

        res
    }

    fn open(&self) {
        let files = &self.files;
        self.gvim.open(&files);
    }

    fn run(&mut self) {
        if !which::which("gvim").unwrap().exists() {
            eprintln!("Error: It seems you don't have gvim executable.");
            std::process::exit(1);
        }

        // check if there's too many arguments
        if self.has_too_many_arguments() {
            std::process::exit(1);
        }

        // split the necessary part of the args.
        let items: Vec<String> = self.args[1..].to_vec();

        let mut count: usize = 0;

        // expand all the items (including internal ones) if each of them is a directory.
        self.files = items
            .iter()
            .take(MAX_FILES)
            .filter_map(|item| {
                let p = PathBuf::from(item);

                // In Windows environment, .canonicalize() returns an abs path with a special prefix \\?\ to express extended-length path.
                // But seemingly this kind of path doens't work properly for gvim so I don't adopt this method.
                // match p.canonicalize() {
                //     Ok(abs_p) => Some(abs_p),
                //     Err(_) => None
                // }

                // We decided not to manipulate specified paths.
                if p.exists() { return Some(p) } else { None }
            })
            .flat_map(|p| expand_dir(p, &mut count))
            .collect();

        // check if total size of the files is small enough to be acceptable
        if self.has_large_size_of_files() {
            std::process::exit(1);
        }

        self.open();

        std::process::exit(0);
    }
}

// Support recursion
fn expand_dir(maybe_dir: PathBuf, count: &mut usize) -> Vec<PathBuf> {
    // if the given argument eventually becomes a file, return the value immediately.
    // is_file will traverse symbolic link.
    if maybe_dir.is_file() {
        let file = maybe_dir;
        *count += 1;
        return vec![file];
    }

    // if the given argument is not readable (i.e. non-directory, lack of permissions) then ignore.
    let Ok(read_dir) = std::fs::read_dir(maybe_dir) else {
        return vec![];
    };

    // expand dir(s)
    let result: Vec<PathBuf> = read_dir
        .into_iter()
        .take(MAX_FILES)
        .filter_map(|entry| {
            match entry {
                Ok(ent) => Some(ent),
                Err(_) => None,
            }
        })
        .flat_map(|ent| {
            *count += 1;

            // we probably never try to handle overcomplicated directory structure with this
            // program so this is sufficient (I don't know).
            if *count > 100 {
                eprintln!("Error: It seems you are trying to expand directories with a complicated structure, but we regard this as an error.\nPlease break down the arguments and perform this program for smaller amount of objects.");
                std::process::exit(1);
            }

            expand_dir(ent.path(), count)
        })
        .collect();

    return result;
}

fn main() {
    let mut app = App::new();
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_to_open_large_file() {
        let mut app = App::new();
        app.files = vec![PathBuf::from("tests/test_asset/huge_file.txt")];
        assert!(app.has_large_size_of_files());
    }

    #[test]
    fn success_to_open_large_file() {
        let mut app = App::new();
        app.files = vec![PathBuf::from("tests/test_asset/huge_file_but_ok.txt")];
        assert!(!app.has_large_size_of_files());
    }
}
