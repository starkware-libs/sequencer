export interface SanitizeOptions {
    /**
     * Do not include empty objects (no keys).
     * @default false
     */
    readonly filterEmptyObjects?: boolean;
    /**
     * Do not include arrays with no items.
     * @default false
     */
    readonly filterEmptyArrays?: boolean;
    /**
     * Sort dictionary keys.
     * @default true
     */
    readonly sortKeys?: boolean;
}
export declare function sanitizeValue(obj: any, options?: SanitizeOptions): any;
