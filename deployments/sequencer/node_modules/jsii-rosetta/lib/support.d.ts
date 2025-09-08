/**
 * Checks whether the current release line is close to End-of-Support (within
 * 30 days), or already in End-of-Support, and if that is the case, emits a
 * warning to call the user to action.
 *
 * It is possible for users to opt out of these notifications by setting the
 * `JSII_SILENCE_SUPPORT_WARNING` environment variable to any truthy value (that
 * is, any non-empty value).
 */
export declare function emitSupportPolicyInformation(): Promise<void>;
//# sourceMappingURL=support.d.ts.map