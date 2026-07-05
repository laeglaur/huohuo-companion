export interface ModelBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
  measuredAt: number;
}

export interface ModelBoundsSet {
  normal?: ModelBounds;
  reactions?: Record<string, ModelBounds>;
  measuredAt?: number;
}
