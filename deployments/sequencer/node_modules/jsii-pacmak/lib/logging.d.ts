export declare enum Level {
    WARN = -1,
    QUIET = 0,
    INFO = 1,
    VERBOSE = 2,
    SILLY = 3
}
export declare const LEVEL_INFO: number;
export declare const LEVEL_VERBOSE: number;
export declare const LEVEL_SILLY: number;
/** The minimal logging level for messages to be emitted. */
export declare let level: Level;
export declare function configure({ level: newLevel }: {
    level: Level;
}): void;
export declare function warn(fmt: string, ...args: any[]): void;
export declare function info(fmt: string, ...args: any[]): void;
export declare function debug(fmt: string, ...args: any[]): void;
//# sourceMappingURL=logging.d.ts.map