"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VersionFile = void 0;
/**
 * Represents the version of the go module. This is needed because the version is not
 * available in standard go module files.
 */
class VersionFile {
    constructor(version) {
        this.version = version;
    }
    emit(code) {
        code.openFile(VersionFile.NAME);
        code.line(this.version);
        code.closeFile(VersionFile.NAME);
    }
}
exports.VersionFile = VersionFile;
VersionFile.NAME = 'version';
//# sourceMappingURL=version-file.js.map