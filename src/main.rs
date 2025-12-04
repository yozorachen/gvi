use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

#[derive(Default)]
enum CheckState {
    #[default]
    NeverChecked,
    CheckedTrue,
    CheckedFalse,
}

#[derive(Default)]
struct GvimState {
    is_instance_exists: CheckState,
    opened_files: usize,
}

// I picked these values off the top of my head
const MAX_ARGS: usize = 20;
const MAX_FILES: usize = 30;
const MAX_SIZE: u64 = 1024 * 300;

impl GvimState {
    fn new() -> Self {
        GvimState::default()
    }

    fn process_check(&mut self) {
        // Here, we only need information of processes.
        // So it's sufficient to initialize the struct without any contents.
        // Then fill in its process field.
        // This lazy init will avoid some overhead at start up,
        // but still, these processes are expensive.
        let mut system = sysinfo::System::new();

        system.refresh_specifics(
            sysinfo::RefreshKind::nothing()
                .with_processes(sysinfo::ProcessRefreshKind::everything()),
        );

        // Let's check if there's already gvim instance or not
        if let Some((_, proc)) = system
            .processes()
            .iter()
            .find(|(_, p)| p.name() == "gvim" || p.name() == "gvim.exe")
        {
            // gvim instance was found.
            //
            // But right after launching gvim, its server functionality isn't fully up and running
            // yet, so simply confirming the process has started is NOT enough!
            //
            // So, to ensure reliable access to the server functions of the gvim instance,
            // we need to wait for a moment.
            //
            // It's uncertain how long we need to wait because it heavily depends on the host
            // machine's specs, but 2 or 3 seconds are sufficient in most cases.
            //
            // run_time() returns "seconds"
            let run_millis = proc.run_time() * 1000;

            const TIME_TO_START_UP_MILLIS: u64 = 2000;

            if run_millis < TIME_TO_START_UP_MILLIS {
                let sleep_millis = TIME_TO_START_UP_MILLIS - run_millis;
                std::thread::sleep(std::time::Duration::from_millis(sleep_millis));
            }

            self.is_instance_exists = CheckState::CheckedTrue;
        } else {
            self.is_instance_exists = CheckState::CheckedFalse;
        }
    }

    fn increment_opened_files(&mut self) {
        self.opened_files += 1;
    }

    fn open_single_item(&mut self, path: &PathBuf) -> Result<(), AppError> {
        if !path.exists() {
            return Err(AppError::ItemPathNotExist(path.clone()));
        }

        match self.is_instance_exists {
            CheckState::NeverChecked | CheckState::CheckedFalse => self.process_check(),
            _ => {}
        }

        match self.is_instance_exists {
            CheckState::CheckedFalse => {
                // if there is no gvim instance, create a new process.
                return self.exec_gvim(&[path]);
            }
            CheckState::CheckedTrue => {
                // if there is at least single gvim instance, use the instance to open the file.
                return self.exec_gvim([
                    OsStr::new("--server-name"),
                    OsStr::new("GVIM"),
                    OsStr::new("--remote-tab"),
                    path.as_ref(),
                ]);
            }
            _ => return Ok(()),
        }
    }

    fn exec_gvim<I, S>(&mut self, args: I) -> Result<(), AppError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        match Command::new("gvim").args(args).spawn() {
            Ok(_) => {
                self.increment_opened_files();
                Ok(())
            }
            Err(e) => Err(AppError::CommandSpawnError(e)),
        }
    }
}

#[derive(Debug)]
enum AppError {
    ItemPathNotExist(PathBuf),
    CommandSpawnError(std::io::Error),
}

struct App {
    args: Vec<String>,
    gvim_state: GvimState,
    listed_files: Vec<PathBuf>,
}

impl App {
    fn new() -> App {
        App {
            args: std::env::args().collect(),
            gvim_state: GvimState::new(),
            listed_files: vec![],
        }
    }

    fn has_files_to_open(&self) -> bool {
        if self.args.len() > 1 {
            return true;
        } else {
            return false;
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

        self.listed_files.iter().for_each(|f| {
            match std::fs::metadata(f) {
                Ok(metadata) => {
                    let size = metadata.len();

                    sum += size;

                    if sum > MAX_SIZE {
                        res = true;
                    }
                }
                Err(_) => {}
            }
        });

        res
    }

    fn run(&mut self) {
        if !which::which("gvim").unwrap().exists() {
            eprintln!("Error: It seems you don't have gvim executable. To begin with, please install that.");
            std::process::exit(1);
        }

        // check if theres's one file or more than that
        if !self.has_files_to_open() {
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
        self.listed_files = items
            .iter()
            .take(MAX_FILES)
            .flat_map(|item| expand_dir(PathBuf::from(item), &mut count))
            .collect();

        // check if total size of the files is small enough to be acceptable
        if self.has_large_size_of_files() {
            std::process::exit(1);
        }

        // try to open each file by using gvim.
        for f in self.listed_files.iter() {
            match self.gvim_state.open_single_item(f) {
                Ok(_) => {}
                Err(e) => match e {
                    AppError::ItemPathNotExist(p) => {
                        eprintln!("Error: Path: {:?} doesn't exist.", p)
                    }
                    AppError::CommandSpawnError(e) => eprintln!("{}", e),
                },
            }
        }
    }
}

// Support recursion
fn expand_dir(dir: PathBuf, count: &mut usize) -> Vec<PathBuf> {
    // if the given argument eventually becomes a file, return the value immediately.
    // is_file will traverse symbolic link.
    if dir.is_file() {
        *count += 1;
        return vec![dir];
    }

    // if the given argument is not readable (i.e. non-directory, lack of permissions) then ignore.
    let Ok(read_dir) = std::fs::read_dir(dir) else {
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
        app.listed_files = vec![PathBuf::from("tests/test_asset/huge_file.txt")];
        assert!(app.has_large_size_of_files());
    }

    #[test]
    fn success_to_open_large_file() {
        let mut app = App::new();
        app.listed_files = vec![PathBuf::from("tests/test_asset/huge_file_but_ok.txt")];
        assert!(!app.has_large_size_of_files());
    }
}
