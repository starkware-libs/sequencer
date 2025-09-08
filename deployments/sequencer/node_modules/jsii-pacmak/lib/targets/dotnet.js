"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DotnetBuilder = exports.TARGET_FRAMEWORK = void 0;
const fs = require("fs-extra");
const path = require("path");
const xmlbuilder = require("xmlbuilder");
const logging = require("../logging");
const target_1 = require("../target");
const util_1 = require("../util");
const dotnetgenerator_1 = require("./dotnet/dotnetgenerator");
const version_utils_1 = require("./version-utils");
const _1 = require(".");
exports.TARGET_FRAMEWORK = 'netcoreapp3.1';
/**
 * Build .NET packages all together, by generating an aggregate solution file
 */
class DotnetBuilder {
    constructor(modules, options) {
        this.modules = modules;
        this.options = options;
        this.targetName = 'dotnet';
    }
    async buildModules() {
        if (this.modules.length === 0) {
            return;
        }
        if (this.options.codeOnly) {
            // Simple, just generate code to respective output dirs
            await Promise.all(this.modules.map((module) => this.generateModuleCode(module, this.outputDir(module.outputDirectory))));
            return;
        }
        // Otherwise make a single tempdir to hold all sources, build them together and copy them back out
        const scratchDirs = [];
        try {
            const tempSourceDir = await this.generateAggregateSourceDir(this.modules);
            scratchDirs.push(tempSourceDir);
            // Build solution
            logging.debug('Building .NET');
            await (0, util_1.shell)('dotnet', ['build', '--force', '--configuration', 'Release'], {
                cwd: tempSourceDir.directory,
                retry: { maxAttempts: 5 },
            });
            await this.copyOutArtifacts(tempSourceDir.object);
            if (this.options.clean) {
                await util_1.Scratch.cleanupAll(scratchDirs);
            }
        }
        catch (e) {
            logging.warn(`Exception occurred, not cleaning up ${scratchDirs
                .map((s) => s.directory)
                .join(', ')}`);
            throw e;
        }
    }
    async generateAggregateSourceDir(modules) {
        return util_1.Scratch.make(async (tmpDir) => {
            logging.debug(`Generating aggregate .NET source dir at ${tmpDir}`);
            const csProjs = [];
            const ret = [];
            // Code generator will make its own subdirectory
            const generatedModules = modules.map((mod) => this.generateModuleCode(mod, tmpDir).then(() => mod));
            for (const mod of await Promise.all(generatedModules)) {
                const loc = projectLocation(mod);
                csProjs.push(loc.projectFile);
                ret.push({
                    outputTargetDirectory: mod.outputDirectory,
                    artifactsDir: path.join(tmpDir, loc.projectDir, 'bin', 'Release'),
                });
            }
            // Use 'dotnet' command line tool to build a solution file from these csprojs
            await (0, util_1.shell)('dotnet', ['new', 'sln', '-n', 'JsiiBuild'], { cwd: tmpDir });
            await (0, util_1.shell)('dotnet', ['sln', 'add', ...csProjs], { cwd: tmpDir });
            await this.generateNuGetConfigForLocalDeps(tmpDir);
            return ret;
        });
    }
    async copyOutArtifacts(packages) {
        logging.debug('Copying out .NET artifacts');
        await Promise.all(packages.map(copyOutIndividualArtifacts.bind(this)));
        async function copyOutIndividualArtifacts(pkg) {
            const targetDirectory = this.outputDir(pkg.outputTargetDirectory);
            await fs.mkdirp(targetDirectory);
            await fs.copy(pkg.artifactsDir, targetDirectory, {
                recursive: true,
                filter: (_, dst) => {
                    return dst !== path.join(targetDirectory, exports.TARGET_FRAMEWORK);
                },
            });
        }
    }
    async generateModuleCode(module, where) {
        const target = this.makeTarget(module);
        logging.debug(`Generating ${this.targetName} code into ${where}`);
        await target.generateCode(where, module.tarball);
    }
    /**
     * Decide whether or not to append 'dotnet' to the given output directory
     */
    outputDir(declaredDir) {
        return this.options.languageSubdirectory
            ? path.join(declaredDir, this.targetName)
            : declaredDir;
    }
    /**
     * Write a NuGet.config that will include build directories for local packages not in the current build
     *
     */
    async generateNuGetConfigForLocalDeps(where) {
        // Traverse the dependency graph of this module and find all modules that have
        // an <outdir>/dotnet directory. We will add those as local NuGet repositories.
        // This enables building against local modules.
        const allDepsOutputDirs = new Set();
        const resolvedModules = this.modules.map(async (module) => ({
            module,
            localBuildDirs: await (0, target_1.findLocalBuildDirs)(module.moduleDirectory, this.targetName),
        }));
        for (const { module, localBuildDirs } of await Promise.all(resolvedModules)) {
            (0, util_1.setExtend)(allDepsOutputDirs, localBuildDirs);
            // Also include output directory where we're building to, in case we build multiple packages into
            // the same output directory.
            allDepsOutputDirs.add(this.outputDir(module.outputDirectory));
        }
        const localRepos = Array.from(allDepsOutputDirs);
        // If dotnet-runtime is checked-out and we can find a local repository, add it to the list.
        try {
            // eslint-disable-next-line @typescript-eslint/no-var-requires,@typescript-eslint/no-require-imports,import/no-extraneous-dependencies
            const jsiiDotNetRuntime = require('@jsii/dotnet-runtime');
            logging.info(`Using local version of the DotNet jsii runtime package at: ${jsiiDotNetRuntime.repository}`);
            localRepos.push(jsiiDotNetRuntime.repository);
        }
        catch {
            // Couldn't locate @jsii/dotnet-runtime, which is owkay!
        }
        // Filter out nonexistant directories, .NET will be unhappy if paths don't exist
        const existingLocalRepos = await (0, util_1.filterAsync)(localRepos, fs.pathExists);
        logging.debug('local NuGet repos:', existingLocalRepos);
        // Construct XML content.
        const configuration = xmlbuilder.create('configuration', {
            encoding: 'UTF-8',
        });
        const packageSources = configuration.ele('packageSources');
        const nugetOrgAdd = packageSources.ele('add');
        nugetOrgAdd.att('key', 'nuget.org');
        nugetOrgAdd.att('value', 'https://api.nuget.org/v3/index.json');
        nugetOrgAdd.att('protocolVersion', '3');
        existingLocalRepos.forEach((repo, index) => {
            const add = packageSources.ele('add');
            add.att('key', `local-${index}`);
            add.att('value', path.join(repo));
        });
        if (this.options.arguments['dotnet-nuget-global-packages-folder']) {
            // Ensure we're not using the configured cache folder
            configuration
                .ele('config')
                .ele('add')
                .att('key', 'globalPackagesFolder')
                .att('value', path.resolve(this.options.arguments['dotnet-nuget-global-packages-folder'], '.nuget', 'packages'));
        }
        const xml = configuration.end({ pretty: true });
        // Write XML content to NuGet.config.
        const filePath = path.join(where, 'NuGet.config');
        logging.debug(`Generated ${filePath}`);
        await fs.writeFile(filePath, xml);
    }
    makeTarget(module) {
        return new Dotnet({
            arguments: this.options.arguments,
            assembly: module.assembly,
            fingerprint: this.options.fingerprint,
            force: this.options.force,
            packageDir: module.moduleDirectory,
            rosetta: this.options.rosetta,
            runtimeTypeChecking: this.options.runtimeTypeChecking,
            targetName: this.targetName,
        }, this.modules.map((m) => m.name));
    }
}
exports.DotnetBuilder = DotnetBuilder;
function projectLocation(module) {
    const packageId = module.assembly.targets.dotnet.packageId;
    return {
        projectDir: packageId,
        projectFile: path.join(packageId, `${packageId}.csproj`),
    };
}
class Dotnet extends target_1.Target {
    static toPackageInfos(assm) {
        const packageId = assm.targets.dotnet.packageId;
        const version = (0, version_utils_1.toReleaseVersion)(assm.version, _1.TargetName.DOTNET);
        const packageInfo = {
            repository: 'Nuget',
            url: `https://www.nuget.org/packages/${packageId}/${version}`,
            usage: {
                csproj: {
                    language: 'xml',
                    code: `<PackageReference Include="${packageId}" Version="${version}" />`,
                },
                dotnet: {
                    language: 'console',
                    code: `dotnet add package ${packageId} --version ${version}`,
                },
                'packages.config': {
                    language: 'xml',
                    code: `<package id="${packageId}" version="${version}" />`,
                },
            },
        };
        return { 'C#': packageInfo };
    }
    static toNativeReference(_type, options) {
        return {
            'c#': `using ${options.namespace};`,
        };
    }
    constructor(options, assembliesCurrentlyBeingCompiled) {
        super(options);
        this.generator = new dotnetgenerator_1.DotNetGenerator(assembliesCurrentlyBeingCompiled, options);
    }
    // eslint-disable-next-line @typescript-eslint/require-await
    async build(_sourceDir, _outDir) {
        throw new Error('Should not be called; use builder instead');
    }
}
exports.default = Dotnet;
//# sourceMappingURL=dotnet.js.map