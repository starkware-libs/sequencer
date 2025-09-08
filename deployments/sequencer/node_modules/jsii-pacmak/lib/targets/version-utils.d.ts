import { TargetName } from '.';
/**
 * Converts a SemVer range expression to a Maven version range expression.
 *
 * @param semverRange the SemVer range expression to convert.
 * @param suffix      the suffix to add to versions in the range.
 *
 * @see https://cwiki.apache.org/confluence/display/MAVENOLD/Dependency+Mediation+and+Conflict+Resolution
 */
export declare function toMavenVersionRange(semverRange: string, suffix?: string): string;
/**
 * Converts a SemVer range expression to a NuGet version range expression.
 *
 * @param semverRange the SemVer range expression to convert.
 *
 * @see https://docs.microsoft.com/en-us/nuget/concepts/package-versioning#version-ranges-and-wildcards
 */
export declare function toNuGetVersionRange(semverRange: string): string;
/**
 * Converts a SemVer range expression to a Python setuptools compatible version
 * constraint expression.
 *
 * @param semverRange the SemVer range expression to convert.
 */
export declare function toPythonVersionRange(semverRange: string): string;
/**
 * Converts an original version number from the NPM convention to the target
 * language's convention for expressing the same. For versions that do not
 * include a prerelease identifier, this always returns the assembly version
 * unmodified.
 *
 * @param assemblyVersion the assembly version being released
 * @param target          the target language for which the version is destined
 *
 * @returns the version that should be serialized
 */
export declare function toReleaseVersion(assemblyVersion: string, target: TargetName): string;
//# sourceMappingURL=version-utils.d.ts.map