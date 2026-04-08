/*!
This module provides facilities to read CSV files in the supported person record format.

Throughout this module, source data files are addressed using two paths:

- the **data path**, configured with [`set_data_path`] and retrieved with [`get_data_path`]
- the **file path**, which is always interpreted relative to that data path

The data path can have one of two forms:

- a path to a directory containing source files in the expected format
- a path to a zip archive containing such files

The same file path is interpreted differently depending on which kind of data path is configured:

- if the data path is a directory, the final source file is `data_path.join(file_path)`
- if the data path is a zip archive, the file path is the name of the file inside the archive

For example, when reading a dataset, if the file path is `all_states/ak.csv`, then:

- with data path `/path/to/ASPR_Synthetic_Population`, the source file is
  `/path/to/ASPR_Synthetic_Population/all_states/ak.csv`
- with data path `/path/to/ASPR_Synthetic_Population.zip`, the source file is the archive member
  named `all_states/ak.csv`

Set and get the data path with the `set_data_path` and `get_data_path` functions:

```rust,ignore
let current_path = get_data_path();
println!("The current data path: {:?}", current_path);

set_data_path(PathBuf::from("../CDC/data/ASPR_Synthetic_Population.zip"));
let new_path = get_data_path();
println!("The new data path: {:?}", new_path);
```

You can iterate over the records in a CSV file by passing a file path relative to the configured
data path to [`PersonRecordIterator::from_path`]:

```ignore
# use crate::pop_reader::archive::PersonRecordIterator;
# use std::path::PathBuf;
let file_path = PathBuf::from("cbsa_all_work_school_household").join("AK/Ketchikan AK.csv");
let records = PersonRecordIterator::from_path(file_path);
// Do something with the records...
```

If the data path is the root of an unzipped dataset, that file path refers to
`cbsa_all_work_school_household/AK/Ketchikan AK.csv` under that directory. If the data path is a zip
archive, the same file path refers to the archive member with that name.

You can get a list of CSV file paths in a given subdirectory with [`iter_csv_files`]. The returned
paths are always relative to the configured data path, so they can be passed directly to
[`PersonRecordIterator::from_path`] or [`PersonRecordIterator::from_file_iterator`]:

```ignore
# use crate::pop_reader::archive::PersonRecordIterator;
# use crate::pop_reader::archive::iter_csv_files;
let records = PersonRecordIterator::from_file_iterator(
    iter_csv_files("all_states").unwrap(),
);
// Do something with the records...
```
*/
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::RwLock,
};

use memchr::memchr;
use once_cell::sync::Lazy;
use ouroboros::self_referencing;
use zip::{read::ZipFile, ZipArchive};

use super::{
    errors::{PopulationReaderError, FIPSParserError},
    parser::{
        parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id, parse_integer,
    },
    PersonRecord,
};

/// The size of the buffer used to read lines from source data files in the expected format. This is
/// chosen to be large enough to keep rebuffering to a minimum, which in turn minimizes the
/// probability that a line spans a buffer window boundary and thus has to be copied to scratch
/// memory for processing.
const READER_CAPACITY: usize = 256 * 1024;
/// The number of fields in each record: age, homeId, schoolId, workplaceId.
const EXPECTED_FIELD_COUNT: usize = 4;

/// Default data path. By default this points at the root of a local unzipped dataset.
const DEFAULT_DATA_PATH: &str = "../CDC/data/ASPR_Synthetic_Population";
// ToDo: Get the data path from an environment variable.
static DATA_PATH: Lazy<RwLock<PathBuf>> =
    Lazy::new(|| RwLock::new(PathBuf::from(DEFAULT_DATA_PATH)));

/// Sets the data path.
///
/// The data path may be either a directory containing source files in the expected format or the
/// path to a zip archive containing such files. File paths accepted elsewhere in this module are
/// always interpreted relative to this data path.
pub fn set_data_path(path: PathBuf) {
    *DATA_PATH.write().unwrap() = path;
}

/// Returns the current data path.
///
/// The returned path is interpreted either as a directory containing source files in the expected
/// format or as the path to a zip archive containing such files. File paths accepted elsewhere in
/// this module are always interpreted relative to this data path.
pub fn get_data_path() -> PathBuf {
    DATA_PATH.read().unwrap().clone()
}

/// Byte-oriented line reader over a particular source data file.
struct FileLineSource {
    reader: BufReader<File>,
    scratch: Vec<u8>,
}

impl FileLineSource {
    fn from_file(file: File) -> Self {
        Self {
            reader: BufReader::with_capacity(READER_CAPACITY, file),
            scratch: Vec::new(),
        }
    }
}

// region ZipLineSource

/// Byte-oriented line reader over a particular source data file within a zip archive.
#[self_referencing]
struct ZipLineSource {
    _archive: ZipArchive<BufReader<File>>,
    scratch: Vec<u8>,

    // This option is always `Some` after successful construction.
    #[borrows(mut _archive)]
    #[covariant]
    line_reader: Option<BufReader<ZipFile<'this, BufReader<File>>>>,
}

impl ZipLineSource {
    /// Constructs a `ZipLineSource` over the lines of the file whose file path is `path` inside the
    /// zip archive at `archive_path`.
    fn from_path(archive_path: PathBuf, path: PathBuf) -> Result<Self, PopulationReaderError> {
        let file = File::open(archive_path).map_err(PopulationReaderError::Io)?;
        let reader = BufReader::with_capacity(READER_CAPACITY, file);
        let mut maybe_error: Option<PopulationReaderError> = None;

        let zip_line_source = ZipLineSourceBuilder {
            _archive: ZipArchive::new(reader).map_err(PopulationReaderError::ZipError)?,
            scratch: Vec::new(),
            line_reader_builder: |archive: &mut ZipArchive<BufReader<File>>| {
                match archive.by_name(path.to_str().unwrap()) {
                    Ok(zipped_file) => Some(BufReader::with_capacity(READER_CAPACITY, zipped_file)),
                    Err(error) => {
                        maybe_error = Some(PopulationReaderError::ZipError(error));
                        None
                    }
                }
            },
        }
        .build();

        match maybe_error {
            Some(error) => Err(error),
            None => Ok(zip_line_source),
        }
    }
}

// endregion ZipLineSource

/// Interface abstracting over the different ways to iterate over lines in a source data file.
enum LineIterator {
    File(FileLineSource),
    Zip(ZipLineSource),
}

impl LineIterator {
    /// Returns a line reader over the file whose file path is `file_path` relative to the current
    /// data path.
    ///
    /// If the data path is a directory, the source file is `data_path.join(file_path)`. If the
    /// data path is a zip archive, `file_path` is the name of the file inside the archive.
    fn from_path(file_path: PathBuf) -> Result<Self, PopulationReaderError> {
        let path = get_data_path();

        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            Ok(Self::Zip(ZipLineSource::from_path(path, file_path)?))
        } else {
            let file = File::open(path.join(file_path)).map_err(PopulationReaderError::Io)?;
            Ok(Self::File(FileLineSource::from_file(file)))
        }
    }

    fn with_next_line<R>(
        &mut self,
        process_line: impl FnOnce(&[u8]) -> Result<R, PopulationReaderError>,
    ) -> Result<Option<R>, PopulationReaderError> {
        match self {

            Self::File(source) => {
                with_next_line_from_reader(&mut source.reader, &mut source.scratch, process_line)
            },

            Self::Zip(source) => {
                source.with_mut(|fields| {
                    let buf_reader = fields.line_reader.as_mut().unwrap();
                    with_next_line_from_reader(buf_reader, fields.scratch, process_line)
                })
            }

        }
    }
}


/// Reads the next line from a buffered reader and processes it using the provided callback
/// function.
///
/// This private helper factors out the common logic of chunking out a slice up to the next newline.
/// In the case that a line spans the boundary of the buffered window, the line is copied into
/// `scratch`, and a slice into `scratch` rather than the underlying buffer is provided to the
/// callback.
fn with_next_line_from_reader<R, Reader>(
    reader: &mut Reader,
    scratch: &mut Vec<u8>,
    process_line: impl FnOnce(&[u8]) -> Result<R, PopulationReaderError>,
) -> Result<Option<R>, PopulationReaderError>
where
    Reader: BufRead,
{
    scratch.clear();

    loop {
        let buf = reader.fill_buf().map_err(PopulationReaderError::Io)?;

        if buf.is_empty() {
            if scratch.is_empty() {
                return Ok(None);
            }

            let line = scratch.as_slice();
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            return process_line(line).map(Some);
        }

        if let Some(newline_index) = memchr(b'\n', buf) {
            if scratch.is_empty() {
                let result = {
                    let line1 = &buf[..newline_index];
                    let line = line1.strip_suffix(b"\r").unwrap_or(line1);
                    process_line(line)
                };
                reader.consume(newline_index + 1);
                return result.map(Some);
            }

            scratch.extend_from_slice(&buf[..newline_index]);
            let result = {
                let line1 = scratch.as_slice();
                let line = line1.strip_suffix(b"\r").unwrap_or(line1);
                process_line(line)
            };
            reader.consume(newline_index + 1);
            return result.map(Some);
        }

        scratch.extend_from_slice(buf);
        let consumed = buf.len();
        reader.consume(consumed);
    }
}

/// Returns an iterator over CSV file paths in the given subdirectory of the current data path.
///
/// `subdirectory` is itself interpreted as a file path relative to the configured data path. The returned paths
/// are always relative to the configured data path as well, regardless of whether the data path is a directory or
/// a zip archive.
pub fn iter_csv_files(
    subdirectory: &'static str,
) -> Result<std::vec::IntoIter<PathBuf>, PopulationReaderError> {
    let mut path = get_data_path();

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        let file = File::open(path).map_err(PopulationReaderError::Io)?;
        let reader = BufReader::with_capacity(READER_CAPACITY, file);
        let archive = ZipArchive::new(reader).map_err(PopulationReaderError::ZipError)?;

        let file_names: Vec<PathBuf> = archive
            .file_names()
            .filter(|name| name.starts_with(subdirectory))
            .map(PathBuf::from)
            .collect();

        Ok(file_names.into_iter())
    } else {
        path.push(subdirectory);
        let entries = path.read_dir().map_err(PopulationReaderError::Io)?;
        let mut files = vec![];

        for entry in entries {
            let entry = entry.map_err(PopulationReaderError::Io)?;
            if entry.path().is_file() {
                files.push(PathBuf::from(subdirectory).join(entry.file_name()));
            }
        }

        Ok(files.into_iter())
    }
}

/// Iterator over person records in a particular data file in the supported person record format.
///
/// The constructors on this type interpret file paths relative to the current data path.
pub struct PersonRecordIterator {
    line_iter: LineIterator,
    line_number: usize,
    failed: bool,
}

/// Used internally by `PersonRecordIterator::from_file_iterator`, FileRecordIterator exists to unify
/// “many records from a file” and “one file-level error for a failed file” into a single iterator
/// type that from_file_iterator can flatten lazily.
enum FileRecordIterator {
    Records(PersonRecordIterator),
    Error(Option<PopulationReaderError>),
}

impl Iterator for FileRecordIterator {
    type Item = Result<PersonRecord, PopulationReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            FileRecordIterator::Records(records) => records.next(),
            FileRecordIterator::Error(error) => error.take().map(Err),
        }
    }
}

impl PersonRecordIterator {
    /// Returns an iterator over the records in the file whose file path is `file_path` relative to the current
    /// data path.
    ///
    /// If the current data path is a directory, the source file is `data_path.join(file_path)`. If the current
    /// data path is a zip archive, `file_path` is the name of the file inside the archive. This function is
    /// intended to be used with [`iter_csv_files`].
    pub fn from_path(file_path: PathBuf) -> Result<Self, PopulationReaderError> {
        let mut line_iter = LineIterator::from_path(file_path.clone())?;

        if line_iter.with_next_line(|_| Ok(()))?.is_none() {
            return Err(PopulationReaderError::EmptyFile(file_path));
        }

        Ok(Self {
            line_iter,
            line_number: 1,
            failed: false,
        })
    }

    /// Returns an iterator over all the rows of all the files in the iterator.
    ///
    /// Each path yielded by `files` is interpreted as a file path relative to the current data
    /// path. If a file cannot be opened as a [`PersonRecordIterator`], this iterator yields
    /// [`PopulationReaderError::FileError`] for that path. This function is intended to be used
    /// with [`iter_csv_files`]:
    ///
    /// ```ignore
    /// # use crate::pop_reader::archive::PersonRecordIterator;
    /// # use crate::pop_reader::archive::iter_csv_files;
    /// let records = PersonRecordIterator::from_file_iterator(iter_csv_files("all_states").unwrap());
    /// ```
    pub fn from_file_iterator(
        files: impl Iterator<Item = PathBuf>,
    ) -> impl Iterator<Item = Result<PersonRecord, PopulationReaderError>> {
        files
            .map(|path| match Self::from_path(path.clone()) {
                Ok(records) => FileRecordIterator::Records(records),
                Err(source) => FileRecordIterator::Error(Some(PopulationReaderError::FileError {
                    path,
                    source: Box::new(source),
                })),
            })
            .flatten()
    }
}

impl Iterator for PersonRecordIterator {
    type Item = Result<PersonRecord, PopulationReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }

        let result = self.line_iter.with_next_line(|line| {
            self.line_number += 1;
            let line_number = self.line_number;
            let mut fields = FieldCursor::new(line, line_number);

            let field = fields.next_field()?;
            let age = parse_optional_field(field, parse_integer)
                .map_err(|source| PopulationReaderError::Parse {
                    field_name: "age",
                    line_number,
                    source,
                })?
                .ok_or(
                    // The case of an empty `age` field, which is an error.
                    PopulationReaderError::Parse {
                        field_name: "age",
                        line_number,
                        source: FIPSParserError::InvalidLength {
                            expected: 1,
                            found: 0,
                        },
                    }
                )?;
            let age = u8::try_from(age).map_err(
                // The case of an `age` field that is too large.
                |_| PopulationReaderError::Parse {
                    field_name: "age",
                    line_number,
                    source: FIPSParserError::ValueExceedsCapacity {
                        value_prefix: age.to_string(),
                        capacity: u64::from(u8::MAX),
                    },
                }
            )?;

            let home_id = parse_optional_field(fields.next_field()?, parse_fips_home_id)
                .map_err(|source| PopulationReaderError::Parse {
                    field_name: "homeId",
                    line_number,
                    source,
                })?;

            let school_id = parse_optional_field(fields.next_field()?, parse_fips_school_id)
                .map_err(|source| PopulationReaderError::Parse {
                    field_name: "schoolId",
                    line_number,
                    source,
                })?;

            let work_id = parse_optional_field(fields.final_field()?, parse_fips_workplace_id)
                .map_err(|source| PopulationReaderError::Parse {
                    field_name: "workplaceId",
                    line_number,
                    source,
                })?;

            Ok(PersonRecord {
                age,
                home_id,
                school_id,
                work_id,
            })
        });

        match result {
            Ok(record) => record.map(Ok),
            Err(error) => {
                if !matches!(
                    error,
                    PopulationReaderError::Parse { .. } | PopulationReaderError::WrongFieldCount { .. }
                ) {
                    self.failed = true;
                }
                Some(Err(error))
            }
        }
    }
}

fn parse_optional_field<T>(
    field: &[u8],
    parser: impl Fn(&[u8]) -> Result<(&[u8], T), (&[u8], FIPSParserError)>,
) -> Result<Option<T>, FIPSParserError> {
    if field.is_empty() {
        return Ok(None);
    }

    let (rest, value) = parser(field).map_err(|(_, source)| source)?;
    if !rest.is_empty() {
        return Err(FIPSParserError::InvalidLength {
            expected: field.len(),
            found: field.len() - rest.len(),
        });
    }

    Ok(Some(value))
}

struct FieldCursor<'a> {
    rest: &'a [u8],
    line_number: usize,
    found: usize,
}

impl<'a> FieldCursor<'a> {
    fn new(line: &'a [u8], line_number: usize) -> Self {
        Self {
            rest: line,
            line_number,
            found: 0,
        }
    }

    /// Returns the next comma-delimited field before the final field, and errors if there are
    /// not enough fields.
    fn next_field(&mut self) -> Result<&'a [u8], PopulationReaderError> {
        let comma = memchr(b',', self.rest).ok_or(PopulationReaderError::WrongFieldCount {
            expected: EXPECTED_FIELD_COUNT,
            found: self.found + 1,
            line_number: self.line_number,
        })?;

        let field = &self.rest[..comma];
        self.rest = &self.rest[comma + 1..];
        self.found += 1;
        Ok(field)
    }

    /// Returns the remaining final field and errors if extra commas remain.
    fn final_field(self) -> Result<&'a [u8], PopulationReaderError> {
        let extra_commas = self.rest.iter().filter(|&&b| b == b',').count();
        if extra_commas != 0 {
            return Err(PopulationReaderError::WrongFieldCount {
                expected: EXPECTED_FIELD_COUNT,
                found: self.found + 1 + extra_commas,
                line_number: self.line_number,
            });
        }

        Ok(self.rest)
    }
}

#[cfg(all(any(feature = "pop_reader_tests", feature = "pop_reader_dataset_tests", feature = "pop_reader_zip_tests"), test))]
mod tests {
    //! These tests assume the existence of data at the default data path and, separately, the existence of the
    //! corresponding zip archive beside it.
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    // Enforce serial execution of tests because the "zip" tests mutate the process-global data path.
    static TEST_MUTEX: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

    #[cfg(feature = "pop_reader_zip_tests")]
    fn test_zip_data_path() -> PathBuf {
        let dataset_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(DEFAULT_DATA_PATH)
            .with_extension("zip");
        assert!(dataset_path.exists());
        dataset_path
    }

    #[cfg(feature = "pop_reader_dataset_tests")]
    fn test_data_path() -> PathBuf {
        let dataset_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_DATA_PATH);
        assert!(dataset_path.exists());
        dataset_path
    }

    fn count_ok_records(
        mut records: impl Iterator<Item = Result<PersonRecord, PopulationReaderError>>,
    ) -> Result<usize, PopulationReaderError> {
        records.try_fold(0usize, |count, record| record.map(|_| count + 1))
    }

    #[test]
    fn test_iter_csv_files_returns_paths_relative_to_directory_data_path() {
        let _guard = TEST_MUTEX.lock();
        let temp_dir = tempdir().unwrap();
        let csv_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&csv_dir).unwrap();
        std::fs::write(csv_dir.join("people.csv"), b"age,homeId,schoolId,workplaceId\n").unwrap();

        set_data_path(temp_dir.path().to_path_buf());

        let files: Vec<PathBuf> = iter_csv_files("subdir").unwrap().collect();
        assert_eq!(files, vec![PathBuf::from("subdir").join("people.csv")]);
    }

    #[test]
    fn test_iter_csv_files_returns_paths_relative_to_zip_data_path() {
        let _guard = TEST_MUTEX.lock();
        let temp_dir = tempdir().unwrap();
        let zip_path = temp_dir.path().join("aspr.zip");
        let zip_file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(zip_file);
        zip_writer
            .start_file("subdir/people.csv", SimpleFileOptions::default())
            .unwrap();
        zip_writer
            .write_all(b"age,homeId,schoolId,workplaceId\n")
            .unwrap();
        zip_writer.finish().unwrap();

        set_data_path(zip_path);

        let files: Vec<PathBuf> = iter_csv_files("subdir").unwrap().collect();
        assert_eq!(files, vec![PathBuf::from("subdir").join("people.csv")]);
    }

    #[test]
    fn test_record_iterator_reports_row_errors_and_recovers_for_directory_input() {
        let _guard = TEST_MUTEX.lock();
        let temp_dir = tempdir().unwrap();
        let csv_path = temp_dir.path().join("people.csv");
        std::fs::write(
            &csv_path,
            concat!(
                "age,homeId,schoolId,workplaceId\n",
                "35,110010109000024,11001009810157,1100100620201546\n",
                "oops,110010109000024,11001009810157,1100100620201546\n",
                "36,110010109000024,11001009810157,1100100620201546\n",
            ),
        )
        .unwrap();

        set_data_path(temp_dir.path().to_path_buf());

        let mut records = PersonRecordIterator::from_path(PathBuf::from("people.csv")).unwrap();

        assert_eq!(records.next().unwrap().unwrap().age, 35);
        assert!(matches!(
            records.next().unwrap(),
            Err(PopulationReaderError::Parse {
                field_name: "age",
                line_number: 3,
                ..
            })
        ));
        assert_eq!(records.next().unwrap().unwrap().age, 36);
        assert!(records.next().is_none());
    }

    #[test]
    fn test_record_iterator_reports_row_errors_and_recovers_for_zip_input() {
        let _guard = TEST_MUTEX.lock();
        let temp_dir = tempdir().unwrap();
        let zip_path = temp_dir.path().join("aspr.zip");
        let zip_file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(zip_file);
        zip_writer
            .start_file("people.csv", SimpleFileOptions::default())
            .unwrap();
        zip_writer
            .write_all(
                concat!(
                    "age,homeId,schoolId,workplaceId\n",
                    "40,110010109000024,11001009810157,1100100620201546\n",
                    "41,110010109000024,11001009810157\n",
                    "42,110010109000024,11001009810157,1100100620201546\n",
                )
                .as_bytes(),
            )
            .unwrap();
        zip_writer.finish().unwrap();

        set_data_path(zip_path);

        let mut records = PersonRecordIterator::from_path(PathBuf::from("people.csv")).unwrap();

        assert_eq!(records.next().unwrap().unwrap().age, 40);
        assert!(matches!(
            records.next().unwrap(),
            Err(PopulationReaderError::WrongFieldCount {
                expected: EXPECTED_FIELD_COUNT,
                found: 3,
                line_number: 3,
            })
        ));
        assert_eq!(records.next().unwrap().unwrap().age, 42);
        assert!(records.next().is_none());
    }

    #[cfg(feature = "pop_reader_dataset_tests")]
    #[test]
    fn test_record_iterator_reads_real_directory_backed_file() {
        let _guard = TEST_MUTEX.lock();
        set_data_path(test_data_path());

        let path = PathBuf::from("cbsa_all_work_school_household").join("AK/Ketchikan AK.csv");
        let records = match PersonRecordIterator::from_path(path) {
            Ok(records) => records,
            Err(error) => panic!("{error:?}"),
        };
        // This count is specific to the local ASPR-compatible dataset fixture.
        assert_eq!(count_ok_records(records).unwrap(), 14132);
    }

    #[cfg(feature = "pop_reader_dataset_tests")]
    #[test]
    fn test_record_iterator_reads_real_directory_backed_files() {
        let _guard = TEST_MUTEX.lock();
        set_data_path(test_data_path());

        let all_path = PathBuf::from("cbsa_all_work_school_household");
        let only_residents_path = PathBuf::from("cbsa_only_residents");
        let paths = vec![
            all_path.join("AK/Ketchikan AK.csv"),
            all_path.join("TX/Vernon TX.csv"),
            only_residents_path.join("AK/Ketchikan AK.csv"),
            only_residents_path.join("TX/Vernon TX.csv"),
        ]
        .into_iter();

        let records = PersonRecordIterator::from_file_iterator(paths);

        // This count is specific to the local ASPR-compatible dataset fixture.
        assert_eq!(count_ok_records(records).unwrap(), 57454);
    }

    #[cfg(feature = "pop_reader_zip_tests")]
    #[test]
    fn test_record_iterator_reads_real_zip_backed_file() {
        let _guard = TEST_MUTEX.lock();
        set_data_path(test_zip_data_path());

        let path = PathBuf::from("cbsa_all_work_school_household").join("AK/Ketchikan AK.csv");
        let records = PersonRecordIterator::from_path(path).unwrap();
        // This count is specific to the local ASPR-compatible dataset fixture.
        assert_eq!(count_ok_records(records).unwrap(), 14132);
    }

    #[cfg(feature = "pop_reader_zip_tests")]
    #[test]
    fn test_record_iterator_reads_real_zip_backed_files() {
        let _guard = TEST_MUTEX.lock();
        set_data_path(test_zip_data_path());

        let all_path = PathBuf::from("cbsa_all_work_school_household");
        let only_residents_path = PathBuf::from("cbsa_only_residents");
        let paths = vec![
            all_path.join("AK/Ketchikan AK.csv"),
            all_path.join("TX/Vernon TX.csv"),
            only_residents_path.join("AK/Ketchikan AK.csv"),
            only_residents_path.join("TX/Vernon TX.csv"),
        ]
        .into_iter();

        let records = PersonRecordIterator::from_file_iterator(paths);

        // This count is specific to the local ASPR-compatible dataset fixture.
        assert_eq!(count_ok_records(records).unwrap(), 57454);
    }
}
