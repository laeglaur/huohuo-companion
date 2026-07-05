declare global {
  interface Window {
    PIXI?: {
      Application: new (options: Record<string, unknown>) => PixiApplication;
      live2d?: {
        Live2DModel?: {
          from: (source: string, options?: Record<string, unknown>) => Promise<Live2DModel>;
        };
      };
    };
    Live2DCubismCore?: unknown;
  }
}

export interface PixiApplication {
  stage: {
    addChild: (child: Live2DModel) => void;
    removeChildren?: () => void;
  };
  renderer: {
    resize: (width: number, height: number) => void;
  };
  destroy: (removeView?: boolean, options?: Record<string, unknown>) => void;
}

export interface Live2DModel {
  anchor?: { set: (x: number, y: number) => void };
  scale: { set: (scale: number) => void };
  position: { set: (x: number, y: number) => void };
  width: number;
  height: number;
  internalModel?: {
    originalWidth?: number;
    originalHeight?: number;
    width?: number;
    height?: number;
    coreModel?: {
      getParameterCount?: () => number;
      getParameterDefaultValue?: (index: number) => number;
      setParameterValueByIndex?: (index: number, value: number, weight?: number) => void;
      saveParameters?: () => void;
    };
    motionManager?: {
      expressionManager?: {
        stopAllExpressions?: () => void;
      };
      _stopAllMotions?: () => void;
    };
    settings?: {
      expressions?: Array<{ Name?: string; name?: string; File?: string; file?: string }>;
      motions?: Record<string, Array<{ File?: string; file?: string }>>;
    };
  };
  expression?: (name?: string) => void | Promise<boolean>;
  motion?: (group?: string, index?: number) => void | Promise<unknown>;
  focus?: (x: number, y: number) => void;
  destroy?: () => void;
}
