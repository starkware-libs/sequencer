export declare enum TargetLanguage {
    PYTHON = "python",
    CSHARP = "csharp",
    JAVA = "java",
    GO = "go"
}
export declare function targetName(language: TargetLanguage): 'python' | 'dotnet' | 'java' | 'go';
export declare function targetName(language: TargetLanguage.PYTHON): 'python';
export declare function targetName(language: TargetLanguage.CSHARP): 'dotnet';
export declare function targetName(language: TargetLanguage.JAVA): 'java';
export declare function targetName(language: TargetLanguage.GO): 'go';
/**
 * @param language a possible value for `TargetLanguage`.
 *
 * @returns the name of the target configuration block for the given language.
 */
export declare function targetName(language: TargetLanguage): 'python' | 'dotnet' | 'java' | 'go';
/**
 * Determines whether the supplied language supports transitive submodule
 * access (similar to how TypeScript/Javascript allows to use a partially
 * qualified name to access a namespace-nested value).
 *
 * If `true`, imports will mirror those found in the original TypeScript
 * code, namespace-traversing property accesses will be rendered as such. This
 * means the following snippet would be transformed "as-is":
 * ```ts
 * import * as cdk from 'aws-cdk-lib';
 * new cdk.aws_s3.Bucket(this, 'Bucket');
 * ```
 *
 * If `false` on the other hand, each used submodule will be imported
 * separately and namespace-traversing property accesses will be replaced with
 * references to the separately-imported submodule. This means the above
 * snippet would be transformed as if it had been modifired to:
 * ```ts
 * import * as aws_s3 from 'aws-cdk-lib/aws-s3';
 * new aws_s3.Bucket(this, 'Bucket');
 * ```
 */
export declare function supportsTransitiveSubmoduleAccess(language: TargetLanguage): boolean;
//# sourceMappingURL=target-language.d.ts.map