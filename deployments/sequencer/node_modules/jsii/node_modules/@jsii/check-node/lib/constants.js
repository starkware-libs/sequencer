"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.NodeRelease = void 0;
const process = require("process");
const semver_1 = require("semver");
const ONE_DAY_IN_MILLISECONDS = 86400000;
/**
 * The support information for a given node release range.
 *
 * @see https://nodejs.org/en/about/releases/
 */
class NodeRelease {
    /**
     * @returns the `NodeRelease` corresponding to the version of the node runtime
     *          executing this code (as provided by `process.version`), and a
     *          boolean indicating whether this version is known to be broken. If
     *          the runtime does not correspond to a known node major, this
     *          returns an `undefined` release object.
     */
    static forThisRuntime() {
        const semver = new semver_1.SemVer(process.version);
        const majorVersion = semver.major;
        for (const nodeRelease of this.ALL_RELEASES) {
            if (nodeRelease.majorVersion === majorVersion) {
                return {
                    nodeRelease,
                    knownBroken: !nodeRelease.supportedRange.test(semver),
                };
            }
        }
        return { nodeRelease: undefined, knownBroken: false };
    }
    /** @internal visible for testing */
    constructor(majorVersion, opts) {
        var _a, _b, _c;
        this.untested = (_a = opts.untested) !== null && _a !== void 0 ? _a : false;
        this.majorVersion = majorVersion;
        this.supportedRange = new semver_1.Range((_b = opts.supportedRange) !== null && _b !== void 0 ? _b : `^${majorVersion}.0.0`);
        this.endOfLifeDate = opts.endOfLife;
        this.endOfLife =
            opts.endOfLife.getTime() + ONE_DAY_IN_MILLISECONDS <= Date.now();
        // jsii EOS defaults to 6 months after EOL
        this.endOfJsiiSupportDate =
            (_c = opts.endOfJsiiSupport) !== null && _c !== void 0 ? _c : new Date(this.endOfLifeDate.getFullYear(), this.endOfLifeDate.getMonth() +
                NodeRelease.DEFAULT_EXTENDED_SUPPORT_MONTHS, this.endOfLifeDate.getDate());
        const endOfJsiiSupport = this.endOfJsiiSupportDate.getTime() + ONE_DAY_IN_MILLISECONDS <=
            Date.now();
        // We deprecate (warn) from EOL to jsii EOS
        this.deprecated = this.endOfLife && !endOfJsiiSupport;
        // All tested and not EOS versions are supported
        this.supported = !this.untested && !endOfJsiiSupport;
    }
    toString() {
        const eolInfo = this.endOfLifeDate
            ? ` (Planned end-of-life: ${this.endOfLifeDate
                .toISOString()
                .slice(0, 10)})`
            : '';
        return `${this.supportedRange.raw}${eolInfo}`;
    }
}
exports.NodeRelease = NodeRelease;
/**
 * How long after end-of-life do we continue to support a node version.
 */
NodeRelease.DEFAULT_EXTENDED_SUPPORT_MONTHS = 6;
/**
 * All registered node releases.
 */
NodeRelease.ALL_RELEASES = [
    // Historical releases (not relevant at time of writing this as they're all EOL now...)
    ...[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((majorVersion) => new NodeRelease(majorVersion, {
        endOfLife: new Date('2018-01-01'),
        untested: true,
    })),
    // Past end-of-life releases
    new NodeRelease(12, {
        endOfLife: new Date('2022-04-30'),
        supportedRange: '^12.7.0',
    }),
    new NodeRelease(13, { endOfLife: new Date('2020-06-01'), untested: true }),
    new NodeRelease(14, {
        endOfLife: new Date('2023-04-30'),
        supportedRange: '^14.17.0',
    }),
    new NodeRelease(15, { endOfLife: new Date('2021-06-01'), untested: true }),
    new NodeRelease(16, {
        endOfLife: new Date('2023-09-11'),
        supportedRange: '^16.3.0',
    }),
    new NodeRelease(17, {
        endOfLife: new Date('2022-06-01'),
        supportedRange: '^17.3.0',
        untested: true,
    }),
    new NodeRelease(19, { endOfLife: new Date('2023-06-01'), untested: true }),
    new NodeRelease(21, { endOfLife: new Date('2024-06-01'), untested: true }),
    new NodeRelease(23, { endOfLife: new Date('2025-06-01'), untested: true }),
    // Currently active releases (as of last edit to this file...)
    new NodeRelease(18, {
        endOfLife: new Date('2025-04-30'),
        endOfJsiiSupport: new Date('2025-11-30'),
    }),
    new NodeRelease(20, { endOfLife: new Date('2026-04-30') }),
    new NodeRelease(22, { endOfLife: new Date('2027-04-30') }),
    new NodeRelease(24, { endOfLife: new Date('2028-04-30') }),
    // Future (planned releases)
];
//# sourceMappingURL=constants.js.map