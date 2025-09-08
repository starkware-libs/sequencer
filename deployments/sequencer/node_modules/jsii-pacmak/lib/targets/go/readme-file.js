"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ReadmeFile = void 0;
class ReadmeFile {
    constructor(packageName, document, directory) {
        this.packageName = packageName;
        this.document = document;
        this.directory = directory;
    }
    emit({ documenter }) {
        if (!this.document) {
            return;
        }
        documenter.emitReadme(this.packageName, this.document, this.directory);
    }
}
exports.ReadmeFile = ReadmeFile;
//# sourceMappingURL=readme-file.js.map