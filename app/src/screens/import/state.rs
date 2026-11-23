//! What the import screen keeps (frontend/src/pages/prompt-import.tsx): the source of each tab, the
//! mapping, the preview and the commit. Simple and Advanced show different controls over this one
//! state, so switching never resets a choice. All fields are signals, so the state is `Copy` and
//! every control reads and writes the same one.

use nsh_core::prelude::*;

use super::helpers::{
    Count, Gate, ImportMode, SourceTab, SummaryState, hf_default_name, limit_error, offset_error, parse_count, read_import_mode,
    split_for_config, strip_extension,
};
use crate::api::ApiError;
use crate::api::types::{
    HfSplit, ImportFormat, ImportMapping, ImportPreviewOut, ImportSource, SplitMode, Turns, UploadOut,
};

/// What a Hugging Face dataset offered, remembered so the selects survive a failed new preview.
#[derive(Debug, Clone, PartialEq)]
pub struct HfInfo {
    pub dataset: String,
    pub splits: Vec<HfSplit>,
    pub config: Option<String>,
    pub split: Option<String>,
}

/// A preview to ask for: the source, the mapping, and what identifies the source (the preview of
/// the same source is kept while the mapping changes).
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub source: ImportSource,
    pub mapping: ImportMapping,
    pub identity: String,
}

#[derive(Clone, Copy)]
pub struct State {
    pub mode: Signal<ImportMode>,
    pub tab: Signal<SourceTab>,

    // Hugging Face.
    pub hf_input: Signal<String>,
    pub hf_dataset: Signal<String>,
    pub hf_config: Signal<Option<String>>,
    pub hf_split: Signal<Option<String>>,
    pub all_splits: Signal<bool>,
    pub hf_info: Signal<Option<HfInfo>>,

    // File.
    pub upload: Signal<Option<UploadOut>>,
    pub uploading: Signal<bool>,
    pub upload_name: Signal<Option<String>>,
    pub upload_error: Signal<Option<String>>,
    pub format: Signal<ImportFormat>,

    // Paste.
    pub paste: Signal<String>,

    // The mapping. `None` = untouched, so the server's detection stays in charge; `Some(None)` is
    // the user's "None".
    pub prompt_field: Signal<Option<Option<String>>>,
    pub system_field: Signal<Option<Option<String>>>,
    pub split_mode: Signal<SplitMode>,
    pub offset: Signal<String>,
    pub limit: Signal<String>,
    pub dedupe: Signal<bool>,
    pub turns: Signal<Turns>,
    pub name: Signal<String>,
    pub name_touched: Signal<bool>,

    // The preview.
    pub preview: Signal<Option<ImportPreviewOut>>,
    pub preview_error: Signal<Option<String>>,
    pub fetching: Signal<bool>,
    /// A change is waiting out its delay before the preview is asked for again.
    pub settling: Signal<bool>,
    /// The tab has something to preview (a dataset, a file, text), whether or not it can be read now.
    pub has_source: Signal<bool>,

    // The commit.
    pub committing: Signal<bool>,
    pub commit_error: Signal<Option<ApiError>>,
}

impl State {
    pub fn new() -> Self {
        State {
            mode: signal(read_import_mode()),
            tab: signal(SourceTab::HuggingFace),
            hf_input: signal(String::new()),
            hf_dataset: signal(String::new()),
            hf_config: signal(None),
            hf_split: signal(None),
            all_splits: signal(false),
            hf_info: signal(None),
            upload: signal(None),
            uploading: signal(false),
            upload_name: signal(None),
            upload_error: signal(None),
            format: signal(ImportFormat::Auto),
            paste: signal(String::new()),
            prompt_field: signal(None),
            system_field: signal(None),
            split_mode: signal(SplitMode::Auto),
            offset: signal(String::new()),
            limit: signal(String::new()),
            dedupe: signal(true),
            turns: signal(Turns::FirstUser),
            name: signal(String::new()),
            name_touched: signal(false),
            preview: signal(None),
            preview_error: signal(None),
            fetching: signal(false),
            settling: signal(false),
            has_source: signal(false),
            committing: signal(false),
            commit_error: signal(None),
        }
    }

    pub fn simple(&self) -> bool {
        self.mode.get() == ImportMode::Simple
    }

    /// Back to what the server detects (a new dataset, config or file).
    pub fn reset_mapping(&self) {
        self.prompt_field.set(None);
        self.system_field.set(None);
        self.split_mode.set(SplitMode::Auto);
        self.offset.set(String::new());
        self.limit.set(String::new());
    }

    pub fn offset_count(&self) -> Count {
        parse_count(&self.offset.get())
    }

    pub fn limit_count(&self) -> Count {
        parse_count(&self.limit.get())
    }

    pub fn offset_error(&self) -> Option<String> {
        offset_error(self.offset_count()).map(str::to_owned)
    }

    pub fn limit_error(&self) -> Option<String> {
        limit_error(self.limit_count()).map(str::to_owned)
    }

    /// The mapping the controls describe.
    pub fn mapping(&self) -> ImportMapping {
        let limit = self.limit_count().value().filter(|n| *n > 0);
        ImportMapping {
            prompt_field: self.prompt_field.get().flatten(),
            // Untouched: let the server detect a system column; "" = explicitly none.
            system_field: match self.system_field.get() {
                None => None,
                Some(None) => Some(String::new()),
                Some(Some(f)) => Some(f),
            },
            split_mode: self.split_mode.get(),
            turns: self.turns.get(),
            offset: self.offset_count().value().unwrap_or(0),
            limit,
            dedupe: self.dedupe.get(),
        }
    }

    /// The preview to show for the current source, or none while there is no source (or a row
    /// limit is invalid).
    pub fn request(&self) -> Option<Request> {
        let source = match self.tab.get() {
            SourceTab::HuggingFace => {
                let dataset = self.hf_dataset.get();
                if dataset.is_empty() {
                    return None;
                }
                let all = self.all_splits.get();
                (
                    ImportSource::Huggingface {
                        dataset: dataset.clone(),
                        config: self.hf_config.get(),
                        split: if all { None } else { self.hf_split.get() },
                        all_splits: all,
                    },
                    format!("hf:{dataset}"),
                )
            }
            SourceTab::File => {
                let upload = self.upload.get()?;
                (ImportSource::File { upload_id: upload.upload_id.clone(), format: self.format.get() }, format!("file:{}", upload.upload_id))
            }
            SourceTab::Paste => {
                let text = self.paste.get();
                if text.trim().is_empty() {
                    return None;
                }
                (ImportSource::Paste { text }, "paste".to_owned())
            }
        };
        if self.offset_error().is_some() || self.limit_error().is_some() {
            return None;
        }
        Some(Request { source: source.0, mapping: self.mapping(), identity: source.1 })
    }

    /// The tab has a source: a dataset, an uploaded file, or text.
    pub fn source_present(&self) -> bool {
        match self.tab.get() {
            SourceTab::HuggingFace => self.hf_dataset.with(|d| !d.is_empty()),
            SourceTab::File => self.upload.with(Option::is_some),
            SourceTab::Paste => self.paste.with(|t| !t.trim().is_empty()),
        }
    }

    /// What the dataset offered, while it is the dataset being imported.
    pub fn info(&self) -> Option<HfInfo> {
        let dataset = self.hf_dataset.get();
        self.hf_info.with(|i| i.clone().filter(|i| i.dataset == dataset))
    }

    /// The config shown: the user's pick, else the one the preview chose.
    pub fn effective_config(&self) -> Option<String> {
        self.hf_config.get().or_else(|| self.info().and_then(|i| i.config))
    }

    /// The split shown: the user's pick, else the one the preview chose.
    pub fn effective_split(&self) -> Option<String> {
        self.hf_split.get().or_else(|| self.info().and_then(|i| i.split))
    }

    /// Picks a config: keeps the split if the config has it, else its default split.
    pub fn pick_config(&self, config: &str) {
        let splits = self.info().map(|i| i.splits).unwrap_or_default();
        let split = split_for_config(&splits, config, self.effective_split().as_deref());
        self.hf_config.set(Some(config.to_owned()));
        self.hf_split.set(split);
        self.reset_mapping();
    }

    /// Picks a split; `None` is every split.
    pub fn pick_split(&self, split: Option<String>) {
        self.hf_config.set(self.effective_config());
        match split {
            None => self.all_splits.set(true),
            Some(s) => {
                self.all_splits.set(false);
                self.hf_split.set(Some(s));
            }
        }
    }

    /// The name the set gets unless the user types one: dataset and split, or the file's name.
    pub fn default_name(&self) -> String {
        match self.tab.get() {
            SourceTab::HuggingFace => {
                hf_default_name(&self.hf_dataset.get(), self.effective_split().as_deref(), self.all_splits.get())
            }
            SourceTab::File => self.upload.with(|u| u.as_ref().map(|u| strip_extension(&u.filename)).unwrap_or_default()),
            SourceTab::Paste => {
                if self.paste.with(|p| p.trim().is_empty()) { String::new() } else { "Pasted prompts".to_owned() }
            }
        }
    }

    /// The name in the field: the user's, once they typed, else the default.
    pub fn shown_name(&self) -> String {
        if self.name_touched.get() { self.name.get() } else { self.default_name() }
    }

    /// The name the set is imported under. Simple mode has no name field: a name typed in Advanced
    /// is kept, an empty one falls back.
    pub fn import_name(&self) -> String {
        let shown = self.shown_name();
        if self.simple() && shown.trim().is_empty() { self.default_name() } else { shown }
    }

    /// Prompts the import will create, when the preview knows.
    pub fn estimated(&self) -> Option<i64> {
        self.preview.with(|p| p.as_ref().and_then(|p| p.estimated_prompts))
    }

    /// What decides whether Import can be pressed.
    pub fn gate(&self) -> Gate {
        let (has_preview, no_prompt_column) = self
            .preview
            .with(|p| (p.is_some(), p.as_ref().is_some_and(|p| !p.columns.is_empty() && p.prompt_field.is_none())));
        Gate {
            has_preview,
            preview_error: self.preview_error.with(Option::is_some),
            estimated: self.estimated(),
            no_prompt_column,
            name_missing: self.import_name().trim().is_empty(),
            offset_error: self.offset_error().is_some(),
            limit_error: self.limit_error().is_some(),
            uploading: self.uploading.get(),
            simple: self.simple(),
        }
    }

    /// The source to import, with the config and split the preview settled on.
    pub fn commit_source(&self) -> Option<ImportSource> {
        match self.tab.get() {
            SourceTab::HuggingFace => {
                let dataset = self.hf_dataset.get();
                if dataset.is_empty() {
                    return None;
                }
                let all = self.all_splits.get();
                let (data_config, data_split) = self.preview.with(|p| p.as_ref().map(|p| (p.hf_config.clone(), p.hf_split.clone())).unwrap_or_default());
                Some(ImportSource::Huggingface {
                    dataset,
                    config: data_config.or_else(|| self.hf_config.get()),
                    split: if all { None } else { data_split.or_else(|| self.hf_split.get()) },
                    all_splits: all,
                })
            }
            SourceTab::File => {
                let upload = self.upload.get()?;
                Some(ImportSource::File { upload_id: upload.upload_id, format: self.format.get() })
            }
            SourceTab::Paste => Some(ImportSource::Paste { text: self.paste.get() }),
        }
    }

    /// The facts Simple mode lists.
    pub fn summary_state(&self) -> SummaryState {
        let default = self.default_name();
        let typed = self.name.get();
        SummaryState {
            source: self.tab.get(),
            turns: self.turns.get(),
            all_splits: self.all_splits.get(),
            format: self.format.get(),
            offset: self.offset_count().value(),
            limit: self.limit_count().value(),
            dedupe: self.dedupe.get(),
            name: (self.name_touched.get() && !typed.trim().is_empty() && typed.trim() != default).then(|| typed.trim().to_owned()),
        }
    }
}
