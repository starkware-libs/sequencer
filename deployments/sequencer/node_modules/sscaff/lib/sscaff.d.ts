/**
 * Copy all files from `templateDir` to `targetDir` and substitute all variables
 * in file names and their contents. Substitutions take the form `{{ key }}`.
 *
 * @param sourceDir
 * @param targetDir
 * @param variables
 */
export declare function sscaff(sourceDir: string, targetDir: string, variables?: {
    [key: string]: string;
}): Promise<void>;
