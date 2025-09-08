import * as spec from '@jsii/spec';
/**
 * Return a fingerprint for a type.
 *
 * The fingerprint will change if the API of the given type changes.
 *
 * The fingerprint is an approximation, it's not exhaustive. It will not trace
 * into types from assemblies it can't see, for example. For the purposes of Rosetta,
 * we'll assume this is Good Enough™.
 */
export declare class TypeFingerprinter {
    private readonly cache;
    private readonly assemblies;
    constructor(assemblies: spec.Assembly[]);
    /**
     * Return a single fingerprint that encompasses all fqns in the list
     */
    fingerprintAll(fqns: string[]): string;
    /**
     * Return the fingerprint for the given FQN in the assembly of this fingerprinter
     *
     * The fingerprint is always going to contain the FQN, even if the type doesn't exist
     * in this assembly.
     */
    fingerprintType(fqn: string): string;
    private doFingerprint;
    private findType;
}
//# sourceMappingURL=fingerprinting.d.ts.map