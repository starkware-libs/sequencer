"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.locationInRepository = locationInRepository;
exports.repositoryUrl = repositoryUrl;
/**
 * Return the repository location for the given API item
 */
function locationInRepository(item) {
    const moduleLoc = item.locationInModule;
    if (!moduleLoc) {
        return undefined;
    }
    const moduleDir = item.assembly.repository.directory;
    if (!moduleDir) {
        return moduleLoc;
    }
    return {
        filename: `${moduleDir}/${moduleLoc.filename}`,
        line: moduleLoc.line,
    };
}
/**
 * Return a URL for this item into the source repository, if available
 *
 * (Currently only supports GitHub URLs)
 */
function repositoryUrl(item, ref = 'master') {
    const loc = locationInRepository(item);
    if (!loc) {
        return undefined;
    }
    const repo = item.assembly.repository;
    if (!repo.url.startsWith('https://') ||
        !repo.url.includes('github.com') ||
        !repo.url.endsWith('.git')) {
        return undefined;
    }
    // Turn https://github.com/awslabs/aws-cdk.git ->  https://github.com/awslabs/aws-cdk/blob/REF/filename#L<number>
    const prefix = repo.url.slice(0, -4);
    return `${prefix}/blob/${ref}/${loc.filename}#L${loc.line}`;
}
//# sourceMappingURL=source.js.map