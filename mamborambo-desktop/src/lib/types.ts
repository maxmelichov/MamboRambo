export type ModelBundle = {
  installed: boolean;
  runtime: "blue";
  model_path: string;
  codec_path: string;
  voices_path?: string;
  espeak_data_path?: string;
  model_dir: string;
  version: string;
  url: string;
};

export type ModelSourceFile = {
  name: string;
  url: string;
};

export type ModelSource = {
  id: "blue";
  name: string;
  version: string;
  size: string;
  description: string;
  files: ModelSourceFile[];
  archive_url?: string | null;
  directory: string;
};

export type ModelSources = {
  runtimes: ModelSource[];
  voices_url: string;
  default_paths: string[];
};

export type RunnerInfo = {
  base_url: string;
};

export type DownloadProgress = {
  downloaded: number;
  total?: number | null;
  progress?: number | null;
  stage: "downloading" | "extracting";
};

export type CreateStep = "idle" | "starting" | "loading" | "creating" | "done";

export type StudioState = {
  text: string;
  referencePath: string;
  languages: string[];
  language: string;
  blueVoice: string;
  blueVoiceIds: string[];
  audioPath: string;
  audioAutoplayPending: boolean;
  step: CreateStep;
  status: string;
  busy: boolean;
  error: string;
};

export type VoicePreset = {
  id: string;
  name: string;
  description: string;
  language: string;
  url: string;
};

export type VoiceCatalog = {
  version: string;
  source: string;
  text: string;
  voices: VoicePreset[];
};

export type DownloadedVoice = {
  path: string;
};
