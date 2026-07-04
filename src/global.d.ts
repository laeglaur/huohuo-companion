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
  };
  expression?: () => void;
  motion?: (group?: string) => void;
  focus?: (x: number, y: number) => void;
  destroy?: () => void;
}
