import { Range } from 'semver';
/**
 * The support information for a given node release range.
 *
 * @see https://nodejs.org/en/about/releases/
 */
export declare class NodeRelease {
    /**
     * How long after end-of-life do we continue to support a node version.
     */
    private static readonly DEFAULT_EXTENDED_SUPPORT_MONTHS;
    /**
     * All registered node releases.
     */
    static readonly ALL_RELEASES: readonly NodeRelease[];
    /**
     * @returns the `NodeRelease` corresponding to the version of the node runtime
     *          executing this code (as provided by `process.version`), and a
     *          boolean indicating whether this version is known to be broken. If
     *          the runtime does not correspond to a known node major, this
     *          returns an `undefined` release object.
     */
    static forThisRuntime(): {
        nodeRelease: NodeRelease | undefined;
        knownBroken: boolean;
    };
    /**
     * The major version of this node release.
     */
    readonly majorVersion: number;
    /**
     * The date on which this release range starts to be considered end-of-life.
     * Defaults to a pre-CDK date for "ancient" releases (before Node 12).
     */
    readonly endOfLifeDate: Date;
    /**
     * Determines whether this release has reached end of support for jsii.
     * This is usually longer then endOfLife;
     */
    readonly endOfJsiiSupportDate: Date;
    /**
     * Determines whether this release is within the deprecation window ahead of
     * it's end-of-life date.
     */
    readonly deprecated: boolean;
    /**
     * Determines whether this release has reached end-of-life.
     */
    readonly endOfLife: boolean;
    /**
     * Determines whether this major version line is currently "in support",
     * meaning it is not end-of-life or within our extended support time frame.
     */
    readonly supported: boolean;
    /**
     * If `true` denotes that this version of node has not been added to the test
     * matrix yet. This is used when adding not-yet-released versions of node that
     * are already planned (typically one or two years out).
     *
     * @default false
     */
    readonly untested: boolean;
    /**
     * The range of versions from this release line that are supported (early
     * releases in a new line often lack essential features, and some have known
     * bugs).
     */
    readonly supportedRange: Range;
    /** @internal visible for testing */
    constructor(majorVersion: number, opts: {
        endOfLife: Date;
        endOfJsiiSupport?: Date;
        untested?: boolean;
        supportedRange?: string;
    });
    toString(): string;
}
//# sourceMappingURL=constants.d.ts.map