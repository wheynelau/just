use super::*;

pub(crate) fn load_dotenv(
  config: &Config,
  settings: &Settings,
  working_directory: &Path,
) -> RunResult<'static, BTreeMap<String, String>> {
  if let Some(script) = settings.dotenv_script.as_ref() {
    if let Some(map) = load_from_script(script, settings, config)? {
      return Ok(map);
    }
  }
  let dotenv_filename = config
    .dotenv_filename
    .as_ref()
    .or(settings.dotenv_filename.as_ref());

  let dotenv_path = config
    .dotenv_path
    .as_ref()
    .or(settings.dotenv_path.as_ref());

  if !settings.dotenv_load
    && !settings.dotenv_override
    && !settings.dotenv_required
    && dotenv_filename.is_none()
    && dotenv_path.is_none()
  {
    return Ok(BTreeMap::new());
  }

  if let Some(path) = dotenv_path {
    let path = working_directory.join(path);
    if let Some(map) = load_from_file(&path, settings)? {
      return Ok(map);
    }
  }

  let filename = dotenv_filename.map_or(".env", |s| s.as_str());

  for directory in working_directory.ancestors() {
    let path = directory.join(filename);
    if let Some(map) = load_from_file(&path, settings)? {
      return Ok(map);
    }
  }

  if settings.dotenv_required {
    Err(Error::DotenvRequired)
  } else {
    Ok(BTreeMap::new())
  }
}

fn load_from_script(
  script: &str,
  settings: &Settings,
  config: &Config,
) -> RunResult<'static, Option<BTreeMap<String, String>>> {
  let mut cmd = settings.shell_command(config);

  cmd
    .arg(script)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

  let (result, caught) = cmd.output_guard();
  let output = result.map_err(|io_error| Error::ShellIo {
    io_error,
    recipe: "(dotenv-script)",
    shell: settings.shell(config).0.into(),
  })?;
  if let Some(signal) = caught {
    return Err(Error::Interrupted { signal });
  }
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    return Err(Error::DotenvScriptExec {
      script: script.to_owned(),
      exit_status: output.status,
      stderr,
    });
  }
  let stdout = std::str::from_utf8(&output.stdout)
    .map_err(OutputError::Utf8)
    .map_err(Error::DotenvScriptOutput)?;
  let mut dotenv = BTreeMap::new();
  for result in dotenvy::from_read_iter(stdout.as_bytes()) {
    let (key, value) = result.map_err(|dotenv_error| Error::Dotenv {
      dotenv_error,
      path: PathBuf::new(),
    })?;

    if settings.dotenv_override || env::var_os(&key).is_none() {
      dotenv.insert(key, value);
    }
  }
  Ok(Some(dotenv))
}

fn load_from_file(
  path: &Path,
  settings: &Settings,
) -> RunResult<'static, Option<BTreeMap<String, String>>> {
  if path.is_dir() {
    return Ok(None);
  }

  let file = match File::open(path) {
    Ok(file) => file,
    Err(source) => {
      if source.kind() == io::ErrorKind::NotFound {
        return Ok(None);
      }
      return Err(Error::FilesystemIo {
        path: path.into(),
        source,
      });
    }
  };

  let mut dotenv = BTreeMap::new();

  for result in dotenvy::from_read_iter(file) {
    let (key, value) = result.map_err(|dotenv_error| Error::Dotenv {
      dotenv_error,
      path: path.into(),
    })?;

    if settings.dotenv_override || env::var_os(&key).is_none() {
      dotenv.insert(key, value);
    }
  }

  Ok(Some(dotenv))
}
