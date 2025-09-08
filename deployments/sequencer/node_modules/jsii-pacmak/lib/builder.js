"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.IndependentPackageBuilder = void 0;
const path = require("path");
const logging = require("./logging");
const util_1 = require("./util");
/**
 * Base implementation, building the package targets for the given language independently of each other
 *
 * Some languages can gain substantial speedup in preparing an "uber project" for all packages
 * and compiling them all in one go (Those will be implementing a custom Builder).
 *
 * For languages where it doesn't matter--or where we haven't figured out how to
 * do that yet--this class can serve as a base class: it will build each package
 * independently, taking care to build them in the right order.
 */
class IndependentPackageBuilder {
    constructor(targetName, targetConstructor, modules, options) {
        this.targetName = targetName;
        this.targetConstructor = targetConstructor;
        this.modules = modules;
        this.options = options;
    }
    async buildModules() {
        if (this.options.codeOnly) {
            await Promise.all((0, util_1.flatten)(this.modules).map((module) => this.generateModuleCode(module, this.options)));
            return;
        }
        for (const modules of this.modules) {
            // eslint-disable-next-line no-await-in-loop
            await Promise.all(modules.map((module) => this.buildModule(module, this.options)));
        }
    }
    async generateModuleCode(module, options) {
        const outputDir = this.finalOutputDir(module, options);
        logging.debug(`Generating ${this.targetName} code into ${outputDir}`);
        await this.makeTarget(module, options).generateCode(outputDir, module.tarball);
    }
    async buildModule(module, options) {
        const target = this.makeTarget(module, options);
        const outputDir = this.finalOutputDir(module, options);
        const src = await util_1.Scratch.make((tmpdir) => {
            logging.debug(`Generating ${this.targetName} code into ${tmpdir}`);
            return target.generateCode(tmpdir, module.tarball);
        });
        try {
            logging.debug(`Building ${src.directory} into ${outputDir}`);
            return await target.build(src.directory, outputDir);
        }
        catch (err) {
            logging.warn(`Failed building ${this.targetName}`);
            // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
            return await Promise.reject(err);
        }
        finally {
            if (options.clean) {
                logging.debug(`Cleaning ${src.directory}`);
                await src.cleanup();
            }
            else {
                logging.info(`Generated code for ${this.targetName} retained at ${src.directory}`);
            }
        }
    }
    makeTarget(module, options) {
        return new this.targetConstructor({
            arguments: options.arguments,
            assembly: module.assembly,
            fingerprint: options.fingerprint,
            force: options.force,
            packageDir: module.moduleDirectory,
            rosetta: options.rosetta,
            runtimeTypeChecking: options.runtimeTypeChecking,
            targetName: this.targetName,
        });
    }
    finalOutputDir(module, options) {
        if (options.languageSubdirectory) {
            return path.join(module.outputDirectory, this.targetName);
        }
        return module.outputDirectory;
    }
}
exports.IndependentPackageBuilder = IndependentPackageBuilder;
//# sourceMappingURL=builder.js.map