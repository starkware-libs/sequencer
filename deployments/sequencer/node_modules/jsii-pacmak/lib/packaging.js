"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.JsiiModule = exports.DEFAULT_PACK_COMMAND = void 0;
const fs = require("fs-extra");
const path = require("path");
const util_1 = require("./util");
const logging = require("../lib/logging");
exports.DEFAULT_PACK_COMMAND = 'npm pack';
class JsiiModule {
    constructor(options) {
        this.name = options.name;
        this.moduleDirectory = options.moduleDirectory;
        this.availableTargets = options.availableTargets;
        this.outputDirectory = options.defaultOutputDirectory;
        this.dependencyNames = options.dependencyNames ?? [];
    }
    /**
     * Prepare an NPM package from this source module
     */
    async npmPack(packCommand = exports.DEFAULT_PACK_COMMAND) {
        this._tarball = await util_1.Scratch.make(async (tmpdir) => {
            const args = [];
            if (packCommand === exports.DEFAULT_PACK_COMMAND) {
                // Quoting (JSON-stringifying) the module directory in order to avoid
                // problems if there are spaces or other special characters in the path.
                args.push(JSON.stringify(this.moduleDirectory));
                if (logging.level.valueOf() >= logging.LEVEL_VERBOSE) {
                    args.push('--loglevel=verbose');
                }
            }
            else {
                // Ensure module is copied to tmpdir to ensure parallel execution does not contend on generated tarballs
                await fs.copy(this.moduleDirectory, tmpdir, { dereference: true });
            }
            const out = await (0, util_1.shell)(packCommand, args, {
                cwd: tmpdir,
            });
            // Take only the last line of npm pack which should contain the
            // tarball name. otherwise, there can be a lot of extra noise there
            // from scripts that emit to STDOUT.
            // Since we are interested in the text *after* the last newline, splitting on '\n' is correct
            // both on Linux/Mac (EOL = '\n') and Windows (EOL = '\r\n'), and also for UNIX tools running
            // on Windows (expected EOL = '\r\n', actual EOL = '\n').
            const lines = out.trim().split('\n');
            const lastLine = lines[lines.length - 1].trim();
            if (!lastLine.endsWith('.tgz') && !lastLine.endsWith('.tar.gz')) {
                throw new Error(`${packCommand} did not produce tarball from ${this.moduleDirectory} into ${tmpdir} (output was ${JSON.stringify(lines.map((l) => l.trimEnd()))})`);
            }
            return path.resolve(tmpdir, lastLine);
        });
    }
    get tarball() {
        if (!this._tarball) {
            throw new Error('Tarball not available yet, call npmPack() first');
        }
        return this._tarball.object;
    }
    async load(system, validate = true) {
        return system
            .loadModule(this.moduleDirectory, { validate })
            .then((assembly) => (this._assembly = assembly));
    }
    get assembly() {
        if (!this._assembly) {
            throw new Error('Assembly not available yet, call load() first');
        }
        return this._assembly;
    }
    async cleanup() {
        if (this._tarball) {
            await this._tarball.cleanup();
        }
    }
}
exports.JsiiModule = JsiiModule;
//# sourceMappingURL=packaging.js.map