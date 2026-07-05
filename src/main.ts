import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize, PhysicalPosition } from "@tauri-apps/api/window";
import "./styles.css";
import type { Live2DModel, PixiApplication } from "./global";

const DEFAULT_MODEL_PATH = "/Users/laeglaur/Documents/code/record/huohuo/huohuo.model3.json";
const BASE_HEIGHT = 420;
const MIN_BASE_WIDTH = 120;
const MAX_BASE_WIDTH = 360;
const WIDTH_PADDING = 18;
const MIN_SCALE = 0.75;
const MAX_SCALE = 1.8;
const PET_HORIZONTAL_INSET = 4;
const PET_VERTICAL_INSET = 10;
const CROP_PADDING = 6;
const MAX_WINDOW_CROP_RATIO = 0.22;
const CROP_ALPHA_THRESHOLD = 8;
const MODEL_BOUNDS_STORAGE_KEY = "huohuo.modelBounds.reactions.v1";
const BOUNDS_SETTLE_FRAMES = 8;
const MOTION_BOUNDS_SAMPLE_FRAMES = 180;
const BUBBLE_GAP = 8;
const BUBBLE_RESERVED_HEIGHT = 54;
const DOG_MODEL_PATH = "/Users/laeglaur/Documents/code/record/anime/Mimi/dog.model3.json";
const DOG_BOUNDS_MARGIN = { left: 0.08, right: 0.32, top: 0.03, bottom: 0.03 };
const BOUNDS_FOCUS_DISTANCE = 9999;
const BOUNDS_FOCUS_POINTS = [
  { x: 0, y: 0 },
  ...Array.from({ length: 12 }, (_, index) => {
    const angle = (Math.PI * 2 * index) / 12;
    return {
      x: Math.cos(angle) * BOUNDS_FOCUS_DISTANCE,
      y: Math.sin(angle) * BOUNDS_FOCUS_DISTANCE,
    };
  }),
];
const MOTION_BOUNDS_FRAMES_PER_FOCUS = Math.max(12, Math.ceil(MOTION_BOUNDS_SAMPLE_FRAMES / BOUNDS_FOCUS_POINTS.length));

interface WindowPosition {
  x: number;
  y: number;
}

interface CompanionSettings {
  selectedModelPath?: string | null;
  position?: WindowPosition | null;
  scale: number;
  modelScales?: Record<string, number>;
  lastArchivePort?: number | null;
}

interface CompanionModel {
  id: string;
  displayName: string;
  modelPath: string;
  folder: string;
  sourceGroup: string;
  isDefault: boolean;
}

interface ModelUrlResult {
  url: string;
  port: number;
}

interface NotebookPageSearchResult {
  pageId: string;
  notebookId: string;
  title: string;
  snippet: string;
}

interface CompanionReaction {
  kind: "motion" | "expression" | "clear";
  name: string;
  group?: string | null;
  expression?: string | null;
}

const appWindow = getCurrentWindow();
let settings: CompanionSettings = {
  selectedModelPath: DEFAULT_MODEL_PATH,
  position: null,
  scale: 1,
  modelScales: {},
  lastArchivePort: null,
};
let models: CompanionModel[] = [];
let pixiApp: PixiApplication | null = null;
let live2dModel: Live2DModel | null = null;
let modelReactions: CompanionReaction[] = [];
let isSwitchingModel = false;
let dragState: { x: number; y: number; moved: boolean } | null = null;
let resizeDrag: { startY: number; startScale: number } | null = null;
let clickTimer = 0;
let lineIndex = 0;
let searchTimer = 0;
let isNotebookSearchComposing = false;
let notebookResults: NotebookPageSearchResult[] = [];
let selectedResultIndex = 0;
let appliedCropTrim = { left: 0, top: 0 };
let currentVisibleBounds: VisibleBounds | null = null;
let reactionResetTimer = 0;
let reactionQueue: ReactionState[] = [];
let lastReactionKey = "";
let activeBoundsKey = "normal";
let boundsCalibrationPromise: Promise<void> | null = null;
let pendingBoundsCalibration: { modelPath: string; source: string; reactions: CompanionReaction[] } | null = null;

type ReactionState = { kind: "normal" } | CompanionReaction;
let reactionState: ReactionState = { kind: "normal" };

interface ModelBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
  measuredAt: number;
}

interface ModelBoundsSet {
  normal?: ModelBounds;
  reactions?: Record<string, ModelBounds>;
  measuredAt?: number;
}

interface VisibleBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

const modelBoundsCache: Record<string, ModelBoundsSet> = {};
const PRECOMPUTED_MODEL_BOUNDS: Record<string, ModelBoundsSet> = {};

function loadStoredModelBounds() {
  try {
    const stored = {
      ...PRECOMPUTED_MODEL_BOUNDS,
      ...JSON.parse(localStorage.getItem(MODEL_BOUNDS_STORAGE_KEY) || "{}") as Record<string, ModelBoundsSet | ModelBounds>,
    };
    Object.entries(stored).forEach(([modelPath, value]) => {
      const boundsSet = normalizeBoundsSet(value);
      if (boundsSet) modelBoundsCache[modelPath] = boundsSet;
    });
  } catch (error) {
    logEvent(`load stored bounds failed: ${String(error)}`);
  }
}

function saveStoredModelBounds() {
  localStorage.setItem(MODEL_BOUNDS_STORAGE_KEY, JSON.stringify(modelBoundsCache));
}

function normalizeBoundsSet(value: unknown): ModelBoundsSet | null {
  if (isValidBounds(value)) return { normal: value, reactions: {}, measuredAt: value.measuredAt };
  if (!value || typeof value !== "object") return null;
  const candidate = value as ModelBoundsSet;
  const next: ModelBoundsSet = { reactions: {}, measuredAt: candidate.measuredAt };
  if (isValidBounds(candidate.normal)) next.normal = candidate.normal;
  Object.entries(candidate.reactions || {}).forEach(([key, bounds]) => {
    if (isValidBounds(bounds)) next.reactions![key] = bounds;
  });
  return next.normal || Object.keys(next.reactions || {}).length ? next : null;
}

function isValidBounds(bounds: unknown): bounds is ModelBounds {
  if (!bounds || typeof bounds !== "object") return false;
  const value = bounds as ModelBounds;
  const parts = [value.left, value.top, value.right, value.bottom, value.width, value.height, value.measuredAt];
  if (!parts.every((part) => Number.isFinite(part))) return false;
  if (value.width < 20 || value.height < 20) return false;
  if (value.left < 0 || value.top < 0 || value.right > value.width || value.bottom > value.height) return false;
  return value.right > value.left && value.bottom > value.top;
}

const lines = [
  "今天也来翻旧信。",
  "Option + → 打开 Archive。",
  "Option + ← 打开 Folia。",
  "Option + F 搜索 Folia。",
  "Option + I 打开 iCity。",
  "Option + ↑ / ↓ 可以切换桌宠。",
  "按住 Option 上下拖拽可以调整大小。",
];

function logEvent(message: string) {
  console.info(message);
  invoke("log_event", { message }).catch(() => undefined);
}

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <main class="companion-shell">
    <div id="bubble" class="bubble">今天也来翻旧信。</div>
    <section id="pet" class="pet" aria-label="桌宠" role="button" tabindex="0">
      <canvas id="live2dCanvas" class="live2d-canvas"></canvas>
      <div id="fallback" class="fallback hidden">Huohuo</div>
    </section>
    <section id="notebookSearch" class="notebook-search hidden" aria-label="搜索 Folia">
      <input id="notebookSearchInput" type="search" placeholder="搜索 Folia page" autocomplete="off" />
      <div id="notebookSearchResults" class="notebook-search-results"></div>
    </section>
  </main>
`;

const shell = document.querySelector<HTMLElement>(".companion-shell")!;
const pet = document.querySelector<HTMLElement>("#pet")!;
const bubble = document.querySelector<HTMLElement>("#bubble")!;
const canvas = document.querySelector<HTMLCanvasElement>("#live2dCanvas")!;
const fallback = document.querySelector<HTMLElement>("#fallback")!;
const notebookSearch = document.querySelector<HTMLElement>("#notebookSearch")!;
const notebookSearchInput = document.querySelector<HTMLInputElement>("#notebookSearchInput")!;
const notebookSearchResults = document.querySelector<HTMLElement>("#notebookSearchResults")!;

async function init() {
  logEvent("frontend init");
  try {
    settings = await invoke<CompanionSettings>("load_settings");
  } catch (error) {
    logEvent(`load_settings failed, using defaults: ${String(error)}`);
    settings = {
      selectedModelPath: DEFAULT_MODEL_PATH,
      position: null,
      scale: 1,
      modelScales: {},
      lastArchivePort: null,
    };
  }
  logEvent(`settings loaded: selected=${settings.selectedModelPath || ""}`);
  settings.modelScales ||= {};
  loadStoredModelBounds();
  if (settings.selectedModelPath && settings.modelScales[settings.selectedModelPath] == null) {
    settings.modelScales[settings.selectedModelPath] = clampScale(settings.scale);
  }
  try {
    models = await invoke<CompanionModel[]>("discover_models");
  } catch (error) {
    logEvent(`discover_models failed: ${String(error)}`);
    models = [];
  }
  logEvent(`discovered models: ${models.length}`);
  if (settings.position) {
    await appWindow.setPosition(new PhysicalPosition(settings.position.x, settings.position.y));
  }
  const selected = settings.selectedModelPath || DEFAULT_MODEL_PATH;
  await applyScale(scaleForModel(selected));
  await loadModel(selected, true);
}

function speak(text: string) {
  bubble.textContent = text;
  positionBubble();
  shell.classList.add("is-speaking");
  window.clearTimeout(Number(shell.dataset.bubbleTimer || 0));
  const timer = window.setTimeout(() => shell.classList.remove("is-speaking"), 2600);
  shell.dataset.bubbleTimer = String(timer);
}

async function applyScale(scale: number) {
  const normalized = clampScale(scale);
  settings.scale = normalized;
  const baseWidth = currentModelBaseWidth();
  const bounds = currentModelBounds();
  const trim = cropTrimForSize(baseWidth * normalized, BASE_HEIGHT * normalized, bounds);
  const width = Math.max(80, baseWidth * normalized - trim.left - trim.right);
  const height = Math.max(140, BASE_HEIGHT * normalized - trim.top - trim.bottom) + BUBBLE_RESERVED_HEIGHT;
  const deltaLeft = Math.round(trim.left - appliedCropTrim.left);
  const deltaTop = Math.round(trim.top - appliedCropTrim.top);
  if (deltaLeft || deltaTop) {
    try {
      const position = await appWindow.outerPosition();
      await appWindow.setPosition(new PhysicalPosition(position.x + deltaLeft, position.y + deltaTop));
    } catch (error) {
      logEvent(`crop position adjust failed: ${String(error)}`);
    }
  }
  appliedCropTrim = { left: trim.left, top: trim.top };
  await appWindow.setSize(new LogicalSize(width, height));
  currentVisibleBounds = visibleBoundsForCurrentWindow();
  window.requestAnimationFrame(refreshLayout);
}

function clampScale(scale: number) {
  return Math.max(MIN_SCALE, Math.min(MAX_SCALE, scale || 1));
}

function scaleForModel(modelPath = settings.selectedModelPath || DEFAULT_MODEL_PATH) {
  return settings.modelScales?.[modelPath] ?? 1;
}

function currentModelBaseWidth() {
  return modelBaseWidth(live2dModel);
}

function modelBaseWidth(model: Live2DModel | null) {
  const width = model?.internalModel?.originalWidth
    || model?.internalModel?.width
    || model?.width
    || 210;
  const height = model?.internalModel?.originalHeight
    || model?.internalModel?.height
    || model?.height
    || 420;
  const ratio = width > 0 && height > 0 ? width / height : 0.5;
  return Math.max(MIN_BASE_WIDTH, Math.min(MAX_BASE_WIDTH, BASE_HEIGHT * ratio + WIDTH_PADDING));
}

function currentModelBounds(boundsKey = activeBoundsKey) {
  const modelPath = settings.selectedModelPath || DEFAULT_MODEL_PATH;
  const boundsSet = modelBoundsCache[modelPath];
  if (!boundsSet) return null;
  if (boundsKey !== "normal") {
    return boundsSet.reactions?.[boundsKey] || boundsSet.normal || null;
  }
  return boundsSet.normal || null;
}

async function setActiveBoundsKey(boundsKey: string) {
  if (activeBoundsKey === boundsKey) {
    currentVisibleBounds = visibleBoundsForCurrentWindow();
    positionBubble();
    return;
  }
  activeBoundsKey = boundsKey;
  await applyScale(scaleForModel());
}

function modelExpressionNames(model: Live2DModel | null) {
  return (model?.internalModel?.settings?.expressions || [])
    .map((expression) => expression.Name || expression.name || "")
    .filter((name) => name.length > 0);
}

function modelMotionGroups(model: Live2DModel | null) {
  const motions = model?.internalModel?.settings?.motions || {};
  return Object.entries(motions)
    .filter(([, definitions]) => Array.isArray(definitions) && definitions.length > 0)
    .map(([group]) => group);
}

function reactionKey(reaction: ReactionState) {
  if (reaction.kind === "motion") return `motion:${reaction.group || reaction.name}`;
  if (reaction.kind === "expression") return `expression:${reaction.expression || reaction.name}`;
  if (reaction.kind === "clear") return `clear:${reaction.name}`;
  return "normal";
}

function shuffleReactions(values: ReactionState[]) {
  const shuffled = [...values];
  for (let index = shuffled.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(Math.random() * (index + 1));
    [shuffled[index], shuffled[swapIndex]] = [shuffled[swapIndex], shuffled[index]];
  }
  if (shuffled.length > 1 && reactionKey(shuffled[0]) === lastReactionKey) {
    [shuffled[0], shuffled[1]] = [shuffled[1], shuffled[0]];
  }
  return shuffled;
}

function availableReactions() {
  return modelReactions.filter((reaction) => !(reaction.kind === "motion" && (reaction.group || reaction.name).toLowerCase() === "idle"));
}

function nextReaction() {
  if (!reactionQueue.length) {
    reactionQueue = shuffleReactions(availableReactions());
  }
  return reactionQueue.shift();
}

function resetModelParametersToDefault() {
  const coreModel = live2dModel?.internalModel?.coreModel;
  const count = coreModel?.getParameterCount?.() || 0;
  for (let index = 0; index < count; index += 1) {
    const defaultValue = coreModel?.getParameterDefaultValue?.(index);
    if (Number.isFinite(defaultValue)) {
      coreModel?.setParameterValueByIndex?.(index, defaultValue as number, 1);
    }
  }
  coreModel?.saveParameters?.();
}

async function resetModelReaction(clearQueue = false) {
  if (!live2dModel) return;
  window.clearTimeout(reactionResetTimer);
  live2dModel.internalModel?.motionManager?._stopAllMotions?.();
  live2dModel.internalModel?.motionManager?.expressionManager?.stopAllExpressions?.();
  resetModelParametersToDefault();
  live2dModel.focus?.(0, 0);
  reactionState = { kind: "normal" };
  if (clearQueue) {
    reactionQueue = [];
    lastReactionKey = "";
  }
  await setActiveBoundsKey("normal");
}

async function enterMotionReaction(reaction: CompanionReaction) {
  if (!live2dModel) return;
  const group = reaction.group || reaction.name;
  await resetModelReaction();
  reactionState = reaction;
  lastReactionKey = reactionKey(reactionState);
  logEvent(`model reaction motion: ${reaction.name} (${group})`);
  await setActiveBoundsKey(reactionKey(reaction));
  void live2dModel.motion?.(group);
  reactionResetTimer = window.setTimeout(() => void resetModelReaction(), 9000);
}

async function enterExpressionReaction(reaction: CompanionReaction) {
  if (!live2dModel) return;
  const expression = reaction.expression || reaction.name;
  await resetModelReaction();
  reactionState = reaction;
  lastReactionKey = reactionKey(reactionState);
  logEvent(`model reaction expression: ${reaction.name} (${expression})`);
  await setActiveBoundsKey(reactionKey(reaction));
  void live2dModel.expression?.(expression);
  reactionResetTimer = window.setTimeout(() => void resetModelReaction(), 3200);
}

function playModelReaction() {
  if (!live2dModel) return;

  try {
    const reaction = nextReaction();
    if (reaction?.kind === "motion") {
      void enterMotionReaction(reaction);
      return;
    }
    if (reaction?.kind === "expression") {
      void enterExpressionReaction(reaction);
      return;
    }
    if (reaction?.kind === "clear") {
      logEvent(`model reaction clear: ${reaction.name}`);
      void resetModelReaction();
      return;
    }

    void resetModelReaction(true);
  } catch (error) {
    logEvent(`model reaction failed: ${String(error)}`);
  }
}

async function sampleModelBounds(app: PixiApplication, model: Live2DModel, frames = BOUNDS_SETTLE_FRAMES) {
  let merged: VisibleBounds | null = null;
  await new Promise<void>((resolve) => {
    let frame = 0;
    const sample = () => {
      const bounds = measureAlphaBounds(app);
      if (bounds) {
        merged = mergeVisibleBounds(merged, bounds);
      }
      frame += 1;
      if (frame < frames) {
        window.requestAnimationFrame(sample);
        return;
      }
      resolve();
    };
    window.requestAnimationFrame(sample);
  });
  return merged;
}

async function sampleAllFocusBounds(app: PixiApplication, model: Live2DModel, framesPerFocus = BOUNDS_SETTLE_FRAMES) {
  let merged: VisibleBounds | null = null;
  for (const point of BOUNDS_FOCUS_POINTS) {
    model.focus?.(point.x, point.y);
    const bounds = await sampleModelBounds(app, model, framesPerFocus);
    if (bounds) merged = mergeVisibleBounds(merged, bounds);
  }
  return merged;
}

function cropTrimForSize(windowWidth: number, windowHeight: number, bounds: ModelBounds | null) {
  if (!bounds) return { left: 0, top: 0, right: 0, bottom: 0 };
  const petWidth = Math.max(1, windowWidth - PET_HORIZONTAL_INSET);
  const petHeight = Math.max(1, windowHeight - BUBBLE_RESERVED_HEIGHT - PET_VERTICAL_INSET);
  const xScale = petWidth / bounds.width;
  const yScale = petHeight / bounds.height;
  const maxHorizontalTrim = windowWidth * MAX_WINDOW_CROP_RATIO;
  const maxVerticalTrim = windowHeight * MAX_WINDOW_CROP_RATIO;
  return {
    left: Math.min(maxHorizontalTrim, Math.max(0, bounds.left - CROP_PADDING) * xScale),
    top: Math.min(maxVerticalTrim, Math.max(0, bounds.top - CROP_PADDING) * yScale),
    right: Math.min(maxHorizontalTrim, Math.max(0, bounds.width - bounds.right - CROP_PADDING) * xScale),
    bottom: Math.min(maxVerticalTrim, Math.max(0, bounds.height - bounds.bottom - CROP_PADDING) * yScale),
  };
}

function visibleBoundsForCurrentWindow(): VisibleBounds | null {
  const bounds = currentModelBounds();
  if (!bounds) return null;
  const rect = pet.getBoundingClientRect();
  if (rect.width < 20 || rect.height < 20) return null;
  const trim = cropTrimForSize(
    currentModelBaseWidth() * scaleForModel(),
    BASE_HEIGHT * scaleForModel() + BUBBLE_RESERVED_HEIGHT,
    bounds,
  );
  const fullWidth = rect.width + trim.left + trim.right;
  const fullHeight = rect.height + trim.top + trim.bottom;
  const xScale = fullWidth / bounds.width;
  const yScale = fullHeight / bounds.height;
  return {
    left: bounds.left * xScale - trim.left,
    top: bounds.top * yScale - trim.top,
    right: bounds.right * xScale - trim.left,
    bottom: bounds.bottom * yScale - trim.top,
    width: rect.width,
    height: rect.height,
  };
}

async function setCurrentModelScale(scale: number, persist = true) {
  const modelPath = settings.selectedModelPath || DEFAULT_MODEL_PATH;
  settings.modelScales ||= {};
  const normalized = clampScale(scale);
  settings.modelScales[modelPath] = normalized;
  await applyScale(normalized);
  if (persist) await saveCurrentSettings();
}

async function saveCurrentSettings() {
  await invoke("save_settings", { settings });
}

async function saveCurrentPosition() {
  const position = await appWindow.outerPosition();
  settings.position = { x: position.x, y: position.y };
  await saveCurrentSettings();
}

async function loadModel(modelPath: string, allowFallback: boolean): Promise<boolean> {
  if (isSwitchingModel) {
    logEvent(`loadModel ignored while switching: ${modelPath}`);
    return false;
  }
  isSwitchingModel = true;
  logEvent(`loadModel start: ${modelPath}`);
  try {
    const { url: source } = await invoke<ModelUrlResult>("model_asset_url", { modelPath });
    logEvent(`model source: ${source}`);
    const Live2DModel = window.PIXI?.live2d?.Live2DModel;
    if (!window.PIXI || !Live2DModel) throw new Error("Live2D runtime is not loaded.");

    const rect = pet.getBoundingClientRect();
    const nextApp = new window.PIXI.Application({
      view: canvas,
      width: rect.width,
      height: rect.height,
      transparent: true,
      autoDensity: true,
      resolution: Math.min(window.devicePixelRatio || 1, 2),
    });
    const nextModel = await Live2DModel.from(source, {
      autoInteract: true,
      autoUpdate: true,
    });

    await resetModelReaction();
    live2dModel?.destroy?.();
    pixiApp?.destroy(false);
    pixiApp = nextApp;
    live2dModel = nextModel;
    pixiApp.stage.addChild(live2dModel);
    fallback.classList.add("hidden");
    canvas.classList.remove("hidden");
    settings.selectedModelPath = modelPath;
    try {
      modelReactions = await invoke<CompanionReaction[]>("model_reactions", { modelPath });
      logEvent(`model reactions ${modelPath}: ${modelReactions.map((reaction) => reactionKey(reaction)).join(", ")}`);
    } catch (error) {
      modelReactions = [];
      logEvent(`model_reactions failed: ${String(error)}`);
    }
    reactionQueue = [];
    lastReactionKey = "";
    activeBoundsKey = "normal";
    await applyScale(scaleForModel(modelPath));
    refreshLayout();
    scheduleBoundsCalibration(modelPath, source);
    await saveCurrentSettings();
    logEvent(`loadModel success: ${modelPath}`);
    return true;
  } catch (error) {
    console.warn("Could not load Live2D model", error);
    logEvent(`loadModel failed: ${modelPath}: ${String(error)}`);
    if (allowFallback && modelPath !== DEFAULT_MODEL_PATH) {
      speak("这个模型没加载起来，先回到 Huohuo。");
      isSwitchingModel = false;
      return loadModel(DEFAULT_MODEL_PATH, false);
    }
    if (!live2dModel) {
      canvas.classList.add("hidden");
      fallback.classList.remove("hidden");
    }
    speak("Live2D 没加载起来。");
    return false;
  } finally {
    isSwitchingModel = false;
  }
}

function scheduleBoundsCalibration(modelPath: string, source: string, reactions = modelReactions) {
  const boundsSet = modelBoundsCache[modelPath];
  const calibrationReactions = reactions.filter((reaction) => reaction.kind === "motion" || reaction.kind === "expression");
  const needsCalibration = !boundsSet?.normal || calibrationReactions.some((reaction) => !boundsSet.reactions?.[reactionKey(reaction)]);
  if (!needsCalibration) return;
  logEvent(`alpha bounds scheduled ${modelPath}`);
  if (boundsCalibrationPromise) {
    pendingBoundsCalibration = { modelPath, source, reactions: calibrationReactions };
    return;
  }
  boundsCalibrationPromise = calibrateModelBounds(modelPath, source, calibrationReactions, true)
    .catch((error) => logEvent(`alpha bounds background failed: ${String(error)}`))
    .finally(() => {
      boundsCalibrationPromise = null;
      const pending = pendingBoundsCalibration;
      pendingBoundsCalibration = null;
      if (pending) scheduleBoundsCalibration(pending.modelPath, pending.source, pending.reactions);
    });
}

function refreshLayout() {
  resizeModel();
}

function resizeModel() {
  if (!pixiApp || !live2dModel) return;
  const rect = pet.getBoundingClientRect();
  pixiApp.renderer.resize(rect.width, rect.height);
  const baseWidth = currentModelBaseWidth() * scaleForModel();
  const baseHeight = BASE_HEIGHT * scaleForModel() + BUBBLE_RESERVED_HEIGHT;
  const trim = cropTrimForSize(baseWidth, baseHeight, currentModelBounds());
  const fullWidth = rect.width + trim.left + trim.right;
  const fullHeight = rect.height + trim.top + trim.bottom;
  const modelWidth = live2dModel.internalModel?.originalWidth || live2dModel.internalModel?.width || live2dModel.width || 3000;
  const modelHeight = live2dModel.internalModel?.originalHeight || live2dModel.internalModel?.height || live2dModel.height || 5000;
  const scale = Math.min((fullWidth * 0.98) / modelWidth, (fullHeight * 0.98) / modelHeight);
  live2dModel.anchor?.set(0.5, 0.5);
  live2dModel.scale.set(scale);
  live2dModel.position.set(fullWidth * 0.5 - trim.left, fullHeight * 0.5 - trim.top);
  currentVisibleBounds = visibleBoundsForCurrentWindow();
  window.requestAnimationFrame(positionBubble);
}

async function calibrateModelBounds(
  modelPath: string,
  source: string,
  reactions: CompanionReaction[],
  applyToCurrentModel = false,
) {
  const previous = modelBoundsCache[modelPath];
  const missingReaction = reactions.some((reaction) => !previous?.reactions?.[reactionKey(reaction)]);
  if (previous?.normal && !missingReaction) {
    if (applyToCurrentModel) {
      currentVisibleBounds = visibleBoundsForCurrentWindow();
      positionBubble();
    }
    return;
  }

  const Live2DModel = window.PIXI?.live2d?.Live2DModel;
  if (!window.PIXI || !Live2DModel) return;
  logEvent(`alpha bounds calibration start ${modelPath}: reactions=${reactions.length}`);

  const calibrationCanvas = document.createElement("canvas");
  calibrationCanvas.className = "bounds-calibration-canvas";
  document.body.appendChild(calibrationCanvas);

  const calibrationApp = new window.PIXI.Application({
    view: calibrationCanvas,
    width: 1,
    height: 1,
    transparent: true,
    autoDensity: true,
    resolution: Math.min(window.devicePixelRatio || 1, 2),
  });

  let calibrationModel: Live2DModel | null = null;
  const nextBoundsSet: ModelBoundsSet = {
    normal: previous?.normal,
    reactions: { ...(previous?.reactions || {}) },
    measuredAt: Date.now(),
  };
  try {
    calibrationModel = await Live2DModel.from(source, {
      autoInteract: false,
      autoUpdate: true,
    });
    calibrationApp.stage.addChild(calibrationModel);
    layoutModelForBounds(calibrationApp, calibrationModel);

    const normalVisible = await sampleAllFocusBounds(calibrationApp, calibrationModel);
    const normal = visibleToModelBounds(modelPath, normalVisible);
    if (normal) {
      nextBoundsSet.normal = normal;
      logEvent(`alpha bounds ${modelPath} normal: ${JSON.stringify(normal)}`);
    }

    for (const reaction of reactions) {
      const key = reactionKey(reaction);
      if (nextBoundsSet.reactions?.[key]) continue;
      await resetCalibrationModel(calibrationModel);
      let reactionVisible: VisibleBounds | null = null;
      if (reaction.kind === "expression") {
        const expression = reaction.expression || reaction.name;
        void calibrationModel.expression?.(expression);
        reactionVisible = await sampleAllFocusBounds(calibrationApp, calibrationModel, BOUNDS_SETTLE_FRAMES);
      } else if (reaction.kind === "motion") {
        const group = reaction.group || reaction.name;
        void calibrationModel.motion?.(group);
        reactionVisible = await sampleAllFocusBounds(calibrationApp, calibrationModel, MOTION_BOUNDS_FRAMES_PER_FOCUS);
      }
      const bounds = visibleToModelBounds(modelPath, reactionVisible);
      if (bounds) {
        nextBoundsSet.reactions![key] = bounds;
        logEvent(`alpha bounds ${modelPath} ${key}: ${JSON.stringify(bounds)}`);
      }
    }
  } catch (error) {
    logEvent(`alpha bounds calibration failed: ${modelPath}: ${String(error)}`);
  } finally {
    calibrationModel?.destroy?.();
    calibrationApp.destroy(false);
    calibrationCanvas.remove();
  }

  if (!nextBoundsSet.normal && !Object.keys(nextBoundsSet.reactions || {}).length) return;
  modelBoundsCache[modelPath] = nextBoundsSet;
  saveStoredModelBounds();
  logEvent(`alpha bounds set ${modelPath}: normal=${Boolean(nextBoundsSet.normal)} reactions=${Object.keys(nextBoundsSet.reactions || {}).length}`);

  if (applyToCurrentModel && settings.selectedModelPath === modelPath) {
    await applyScale(scaleForModel(modelPath));
    refreshLayout();
  }
}

function visibleToModelBounds(modelPath: string, bounds: VisibleBounds | null): ModelBounds | null {
  if (!bounds) return null;
  return boundsFromVisibleBounds(expandVisibleBoundsForModel(modelPath, bounds));
}

async function resetCalibrationModel(model: Live2DModel) {
  model.internalModel?.motionManager?._stopAllMotions?.();
  model.internalModel?.motionManager?.expressionManager?.stopAllExpressions?.();
  const coreModel = model.internalModel?.coreModel;
  const count = coreModel?.getParameterCount?.() || 0;
  for (let index = 0; index < count; index += 1) {
    const defaultValue = coreModel?.getParameterDefaultValue?.(index);
    if (Number.isFinite(defaultValue)) {
      coreModel?.setParameterValueByIndex?.(index, defaultValue as number, 1);
    }
  }
  coreModel?.saveParameters?.();
  model.focus?.(0, 0);
  await sampleModelIdleFrame();
}

async function sampleModelIdleFrame() {
  await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
}

function layoutModelForBounds(app: PixiApplication, model: Live2DModel) {
  const baseWidth = modelBaseWidth(model);
  const width = Math.max(1, baseWidth - PET_HORIZONTAL_INSET);
  const height = Math.max(1, BASE_HEIGHT - PET_VERTICAL_INSET);
  const rendererView = (app.renderer as unknown as { view?: HTMLCanvasElement }).view;
  if (rendererView) {
    rendererView.style.width = `${width}px`;
    rendererView.style.height = `${height}px`;
  }
  app.renderer.resize(width, height);
  const modelWidth = model.internalModel?.originalWidth || model.internalModel?.width || model.width || 3000;
  const modelHeight = model.internalModel?.originalHeight || model.internalModel?.height || model.height || 5000;
  const scale = Math.min((width * 0.98) / modelWidth, (height * 0.98) / modelHeight);
  model.anchor?.set(0.5, 0.5);
  model.scale.set(scale);
  model.position.set(width * 0.5, height * 0.5);
}

function renderApp(app: PixiApplication) {
  const renderer = app.renderer as unknown as { render?: (stage: unknown) => void };
  renderer.render?.(app.stage);
}

function measureAlphaBounds(app: PixiApplication): VisibleBounds | null {
  const rendererView = (app.renderer as unknown as { view?: HTMLCanvasElement }).view || canvas;
  const logicalWidth = rendererView.clientWidth || rendererView.width || 0;
  const logicalHeight = rendererView.clientHeight || rendererView.height || 0;
  if (logicalWidth < 20 || logicalHeight < 20) return null;

  try {
    const renderer = app.renderer as unknown as {
      gl?: WebGLRenderingContext | WebGL2RenderingContext;
      context?: { gl?: WebGLRenderingContext | WebGL2RenderingContext };
      view?: HTMLCanvasElement;
    };
    renderApp(app);
    const gl = renderer.gl || renderer.context?.gl || renderer.view?.getContext("webgl2") || renderer.view?.getContext("webgl");
    if (!gl) return null;
    const width = gl.drawingBufferWidth;
    const height = gl.drawingBufferHeight;
    if (!width || !height) return null;
    const pixels = new Uint8Array(width * height * 4);
    gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);

    let minX = width;
    let minY = height;
    let maxX = -1;
    let maxY = -1;
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        if (pixels[(y * width + x) * 4 + 3] <= CROP_ALPHA_THRESHOLD) continue;
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
    if (maxX < minX || maxY < minY) return null;

    const ratioX = width / logicalWidth;
    const ratioY = height / logicalHeight;
    return {
      left: minX / ratioX,
      top: (height - 1 - maxY) / ratioY,
      right: (maxX + 1) / ratioX,
      bottom: (height - minY) / ratioY,
      width: logicalWidth,
      height: logicalHeight,
    };
  } catch (error) {
    logEvent(`alpha bounds failed: ${String(error)}`);
    return null;
  }
}

function mergeVisibleBounds(previous: VisibleBounds | null, next: VisibleBounds) {
  if (!previous) return next;
  return {
    left: Math.min(previous.left, next.left),
    top: Math.min(previous.top, next.top),
    right: Math.max(previous.right, next.right),
    bottom: Math.max(previous.bottom, next.bottom),
    width: next.width,
    height: next.height,
  };
}

function expandVisibleBoundsForModel(modelPath: string, bounds: VisibleBounds): VisibleBounds {
  if (modelPath !== DOG_MODEL_PATH) return bounds;
  return {
    left: Math.max(0, bounds.left - bounds.width * DOG_BOUNDS_MARGIN.left),
    top: Math.max(0, bounds.top - bounds.height * DOG_BOUNDS_MARGIN.top),
    right: Math.min(bounds.width, bounds.right + bounds.width * DOG_BOUNDS_MARGIN.right),
    bottom: Math.min(bounds.height, bounds.bottom + bounds.height * DOG_BOUNDS_MARGIN.bottom),
    width: bounds.width,
    height: bounds.height,
  };
}

function boundsFromVisibleBounds(bounds: VisibleBounds): ModelBounds | null {
  if (bounds.width < 20 || bounds.height < 20) return null;
  return {
    left: bounds.left,
    top: bounds.top,
    right: bounds.right,
    bottom: bounds.bottom,
    width: bounds.width,
    height: bounds.height,
    measuredAt: Date.now(),
  };
}

function positionBubble() {
  const petRect = pet.getBoundingClientRect();
  const bubbleWidth = Math.min(180, Math.max(110, window.innerWidth - 12));
  const bounds = currentVisibleBounds;
  let left = 6;
  let top = 6;
  if (bounds) {
    const visible = {
      left: petRect.left + bounds.left,
      right: petRect.left + bounds.right,
    };
    const visibleWidth = Math.max(1, visible.right - visible.left);
    left = Math.max(6, Math.min(window.innerWidth - bubbleWidth - 6, visible.left + visibleWidth * 0.04));
  } else if (petRect.width > 0) {
    left = Math.max(6, Math.min(window.innerWidth - bubbleWidth - 6, petRect.left + 4));
  }
  bubble.style.left = `${Math.round(left)}px`;
  bubble.style.top = `${Math.round(top)}px`;
  bubble.style.maxWidth = `${Math.round(bubbleWidth)}px`;
}

function blurActiveElementIn(container: HTMLElement) {
  const activeElement = document.activeElement;
  if (activeElement instanceof HTMLElement && container.contains(activeElement)) {
    activeElement.blur();
  }
}

function hideNotebookSearch() {
  blurActiveElementIn(notebookSearch);
  window.clearTimeout(searchTimer);
  isNotebookSearchComposing = false;
  notebookSearch.classList.add("hidden");
  notebookSearchInput.value = "";
  notebookResults = [];
  selectedResultIndex = 0;
  notebookSearchResults.innerHTML = "";
}

function showNotebookSearch() {
  shell.classList.remove("is-speaking");
  notebookSearch.classList.remove("hidden");
  notebookSearchInput.focus({ preventScroll: true });
  notebookSearchInput.select();
}

function renderNotebookResults() {
  notebookSearchResults.innerHTML = "";
  notebookResults.forEach((result, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "notebook-result";
    button.classList.toggle("is-selected", index === selectedResultIndex);
    button.innerHTML = `
      <span>${escapeHtml(result.title || "Untitled")}</span>
      <small>${escapeHtml(stripHtml(result.snippet || ""))}</small>
    `;
    button.addEventListener("click", () => {
      selectedResultIndex = index;
      void openSelectedNotebookResult();
    });
    notebookSearchResults.appendChild(button);
  });
}

async function searchNotebookPages(query: string) {
  const trimmed = query.trim();
  if (!trimmed) {
    notebookResults = [];
    selectedResultIndex = 0;
    renderNotebookResults();
    return;
  }
  try {
    notebookResults = await invoke<NotebookPageSearchResult[]>("search_notebook_pages", { query: trimmed, limit: 8 });
    selectedResultIndex = 0;
    renderNotebookResults();
  } catch (error) {
    notebookResults = [];
    selectedResultIndex = 0;
    notebookSearchResults.innerHTML = `<p>${escapeHtml(String(error))}</p>`;
  }
}

async function openSelectedNotebookResult() {
  const result = notebookResults[selectedResultIndex];
  if (!result) return;
  speak(`打开 ${result.title || "Untitled"} 的便签。`);
  try {
    await invoke("open_notebook_card", { pageId: result.pageId });
    hideNotebookSearch();
  } catch (error) {
    speak(`便签打不开：${String(error)}`);
  }
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;",
  }[char] || char));
}

function stripHtml(value: string) {
  const container = document.createElement("div");
  container.innerHTML = value;
  return (container.textContent || "").replace(/\s+/g, " ").trim();
}

function currentModelIndex() {
  const selected = settings.selectedModelPath || DEFAULT_MODEL_PATH;
  const selectedIndex = models.findIndex((model) => model.modelPath === selected);
  if (selectedIndex >= 0) return selectedIndex;
  const defaultIndex = models.findIndex((model) => model.modelPath === DEFAULT_MODEL_PATH);
  return defaultIndex >= 0 ? defaultIndex : 0;
}

async function switchModel(direction: -1 | 1) {
  if (isSwitchingModel) return;
  if (models.length < 2) {
    speak("没有其他桌宠可以切换。");
    return;
  }
  const index = currentModelIndex();
  const next = models[(index + direction + models.length) % models.length];
  const loaded = await loadModel(next.modelPath, true);
  if (loaded && settings.selectedModelPath === next.modelPath) {
    speak(`换成 ${next.displayName}。`);
  }
}

async function openArchive() {
  hideNotebookSearch();
  speak("Archive 正在打开。");
  try {
    await invoke("launch_archive");
    speak("Archive 已打开。");
  } catch (error) {
    speak(`Archive 打不开：${String(error)}`);
  }
}

async function openNotebook() {
  hideNotebookSearch();
  speak("Folia 正在打开。");
  try {
    await invoke("open_notebook");
    speak("Folia 已打开。");
  } catch (error) {
    speak(`Folia 打不开：${String(error)}`);
  }
}

async function openIcity() {
  hideNotebookSearch();
  speak("iCity 正在打开。");
  try {
    await invoke("open_icity_login");
    speak("iCity 已打开。");
  } catch (error) {
    speak(`iCity 打不开：${String(error)}`);
  }
}

async function resetPosition() {
  hideNotebookSearch();
  try {
    await invoke("reset_position");
    settings.position = { x: 60, y: 160 };
    speak("位置已重置。");
  } catch (error) {
    speak(`重置失败：${String(error)}`);
  }
}

pet.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  pet.focus({ preventScroll: true });
  hideNotebookSearch();
  if (event.altKey) {
    event.preventDefault();
    resizeDrag = {
      startY: event.clientY,
      startScale: scaleForModel(),
    };
    pet.setPointerCapture(event.pointerId);
    document.body.classList.add("is-resizing");
    return;
  }
  dragState = { x: event.clientX, y: event.clientY, moved: false };
});

pet.addEventListener("pointermove", (event) => {
  if (resizeDrag) {
    const delta = (event.clientY - resizeDrag.startY) / 180;
    void setCurrentModelScale(resizeDrag.startScale + delta, false);
    return;
  }
  if (!dragState) return;
  const moved = Math.abs(event.clientX - dragState.x) + Math.abs(event.clientY - dragState.y);
  if (moved > 7) {
    dragState.moved = true;
    appWindow.startDragging().catch(() => undefined);
  }
  live2dModel?.focus?.(event.clientX - pet.offsetLeft, event.clientY - pet.offsetTop);
});

async function finishResizeDrag(event: PointerEvent) {
  if (!resizeDrag) return false;
  if (pet.hasPointerCapture(event.pointerId)) pet.releasePointerCapture(event.pointerId);
  resizeDrag = null;
  document.body.classList.remove("is-resizing");
  await saveCurrentSettings();
  speak(`大小 ${Math.round(scaleForModel() * 100)}%。`);
  return true;
}

pet.addEventListener("pointerup", async (event) => {
  if (await finishResizeDrag(event)) return;
  const wasMoved = dragState?.moved;
  dragState = null;
  if (wasMoved) {
    await saveCurrentPosition();
    return;
  }
  window.clearTimeout(clickTimer);
  clickTimer = window.setTimeout(() => {
    lineIndex = (lineIndex + 1) % lines.length;
    speak(lines[lineIndex]);
    playModelReaction();
  }, 210);
});

pet.addEventListener("pointercancel", async (event) => {
  await finishResizeDrag(event);
  dragState = null;
});

pet.addEventListener("dblclick", (event) => {
  event.preventDefault();
  window.clearTimeout(clickTimer);
});

pet.addEventListener("contextmenu", (event) => {
  event.preventDefault();
});

pet.addEventListener("keydown", async (event) => {
  if (event.key === " ") {
    event.preventDefault();
    lineIndex = (lineIndex + 1) % lines.length;
    speak(lines[lineIndex]);
  }
});

document.addEventListener("pointerdown", (event) => {
  const target = event.target as Node;
  if (!notebookSearch.contains(target) && !pet.contains(target)) {
    hideNotebookSearch();
  }
});

document.addEventListener("keydown", async (event) => {
  const target = event.target as HTMLElement | null;
  const isTyping = Boolean(target?.closest("input, textarea, select, [contenteditable='true']"));
  const key = event.key.toLowerCase();
  if (event.altKey && (event.code === "KeyF" || key === "f")) {
    event.preventDefault();
    showNotebookSearch();
    return;
  }
  if (event.altKey && (event.code === "KeyI" || key === "i")) {
    event.preventDefault();
    await openIcity();
    return;
  }
  if (event.altKey && !isTyping && event.key === "ArrowLeft") {
    event.preventDefault();
    if (event.repeat) return;
    await openNotebook();
    return;
  }
  if (event.altKey && !isTyping && event.key === "ArrowRight") {
    event.preventDefault();
    if (event.repeat) return;
    await openArchive();
    return;
  }
  if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
    event.preventDefault();
    if (event.repeat) return;
    await switchModel(event.key === "ArrowUp" ? -1 : 1);
    return;
  }
  if (event.key === "Escape") {
    hideNotebookSearch();
  }
});

notebookSearchInput.addEventListener("compositionstart", () => {
  isNotebookSearchComposing = true;
});

notebookSearchInput.addEventListener("compositionend", () => {
  isNotebookSearchComposing = false;
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    void searchNotebookPages(notebookSearchInput.value);
  }, 40);
});

notebookSearchInput.addEventListener("input", (event) => {
  if (isNotebookSearchComposing || (event instanceof InputEvent && event.isComposing)) return;
  window.clearTimeout(searchTimer);
  searchTimer = window.setTimeout(() => {
    void searchNotebookPages(notebookSearchInput.value);
  }, 160);
});

notebookSearchInput.addEventListener("keydown", async (event) => {
  if (isNotebookSearchComposing || event.isComposing || event.key === "Process") return;
  if (event.key === "Escape") {
    event.preventDefault();
    hideNotebookSearch();
    pet.focus({ preventScroll: true });
    return;
  }
  if (event.key === "ArrowDown") {
    event.preventDefault();
    selectedResultIndex = Math.min(notebookResults.length - 1, selectedResultIndex + 1);
    renderNotebookResults();
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    selectedResultIndex = Math.max(0, selectedResultIndex - 1);
    renderNotebookResults();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    await openSelectedNotebookResult();
  }
});

window.addEventListener("blur", () => {
  document.body.classList.remove("is-resizing");
  resizeDrag = null;
});
window.addEventListener("resize", refreshLayout);

init().catch((error) => {
  console.error(error);
  speak(`启动失败：${String(error)}`);
});
