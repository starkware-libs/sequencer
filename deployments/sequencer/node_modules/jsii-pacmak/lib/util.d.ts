import { SpawnOptions } from 'child_process';
/**
 * Find the directory that contains a given dependency, identified by its 'package.json', from a starting search directory
 *
 * (This code is duplicated among jsii/jsii-pacmak/jsii-reflect. Changes should be done in all
 * 3 locations, and we should unify these at some point: https://github.com/aws/jsii/issues/3236)
 */
export declare function findDependencyDirectory(dependencyName: string, searchStart: string): Promise<string>;
/**
 * Whether the given dependency is a built-in
 *
 * Some dependencies that occur in `package.json` are also built-ins in modern Node
 * versions (most egregious example: 'punycode'). Detect those and filter them out.
 */
export declare function isBuiltinModule(depName: string): any;
/**
 * Find the package.json for a given package upwards from the given directory
 *
 * (This code is duplicated among jsii/jsii-pacmak/jsii-reflect. Changes should be done in all
 * 3 locations, and we should unify these at some point: https://github.com/aws/jsii/issues/3236)
 */
export declare function findPackageJsonUp(packageName: string, directory: string): Promise<string | undefined>;
/**
 * Find a directory up the tree from a starting directory matching a condition
 *
 * Will return `undefined` if no directory matches
 *
 * (This code is duplicated among jsii/jsii-pacmak/jsii-reflect. Changes should be done in all
 * 3 locations, and we should unify these at some point: https://github.com/aws/jsii/issues/3236)
 */
export declare function findUp(directory: string, pred: (dir: string) => Promise<boolean>): Promise<string | undefined>;
export interface RetryOptions {
    /**
     * The maximum amount of attempts to make.
     *
     * @default 5
     */
    maxAttempts?: number;
    /**
     * The amount of time (in milliseconds) to wait after the first failed attempt.
     *
     * @default 150
     */
    backoffBaseMilliseconds?: number;
    /**
     * The multiplier to apply after each failed attempts. If the backoff before
     * the previous attempt was `B`, the next backoff is computed as
     * `B * backoffMultiplier`, creating an exponential series.
     *
     * @default 2
     */
    backoffMultiplier?: number;
    /**
     * An optionnal callback that gets invoked when an attempt failed. This can be
     * used to give the user indications of what is happening.
     *
     * This callback must not throw.
     *
     * @param error               the error that just occurred
     * @param attemptsLeft        the number of attempts left
     * @param backoffMilliseconds the amount of milliseconds of back-off that will
     *                            be awaited before making the next attempt (if
     *                            there are attempts left)
     */
    onFailedAttempt?: (error: unknown, attemptsLeft: number, backoffMilliseconds: number) => void;
}
export declare class AllAttemptsFailed<R> extends Error {
    readonly callback: () => Promise<R>;
    readonly errors: readonly Error[];
    constructor(callback: () => Promise<R>, errors: readonly Error[]);
}
/**
 * Adds back-off and retry logic around the provided callback.
 *
 * @param cb   the callback which is to be retried.
 * @param opts the backoff-and-retry configuration
 *
 * @returns the result of `cb`
 */
export declare function retry<R>(cb: () => Promise<R>, opts?: RetryOptions, waiter?: (ms: number) => Promise<void>): Promise<R>;
export interface ShellOptions extends Omit<SpawnOptions, 'shell' | 'stdio'> {
    /**
     * Configure in-line retries if the execution fails.
     *
     * @default - no retries
     */
    readonly retry?: RetryOptions;
}
/**
 * Spawns a child process with the provided command and arguments. The child
 * process is always spawned using `shell: true`, and the contents of
 * `process.env` is used as the initial value of the `env` spawn option (values
 * provided in `options.env` can override those).
 *
 * @param cmd     the command to shell out to.
 * @param args    the arguments to provide to `cmd`
 * @param options any options to pass to `spawn`
 */
export declare function shell(cmd: string, args: string[], { retry: retryOptions, ...options }?: ShellOptions): Promise<string>;
/**
 * Strip filesystem unsafe characters from a string
 */
export declare function slugify(x: string): string;
/**
 * Class that makes a temporary directory and holds on to an operation object
 */
export declare class Scratch<A> {
    readonly directory: string;
    readonly object: A;
    private readonly fake;
    static make<A>(factory: (dir: string) => Promise<A>): Promise<Scratch<A>>;
    static make<A>(factory: (dir: string) => A): Promise<Scratch<A>>;
    static fake<A>(directory: string, object: A): Scratch<A>;
    static cleanupAll<A>(tempDirs: Array<Scratch<A>>): Promise<void>;
    private constructor();
    cleanup(): Promise<void>;
}
export declare function setExtend<A>(xs: Set<A>, els: Iterable<A>): void;
export declare function filterAsync<A>(xs: A[], pred: (x: A) => Promise<boolean>): Promise<A[]>;
export declare function wait(ms: number): Promise<void>;
export declare function flatten<A>(xs: readonly A[][]): A[];
//# sourceMappingURL=util.d.ts.map