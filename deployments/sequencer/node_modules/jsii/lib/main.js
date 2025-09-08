"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
require("@jsii/check-node/run");
const path = require("node:path");
const util = require("node:util");
const log4js = require("log4js");
const package_json_1 = require("typescript/package.json");
const yargs = require("yargs");
const compiler_1 = require("./compiler");
const jsii_diagnostic_1 = require("./jsii-diagnostic");
const project_info_1 = require("./project-info");
const support_1 = require("./support");
const tsconfig_1 = require("./tsconfig");
const utils = require("./utils");
const version_1 = require("./version");
const warnings_1 = require("./warnings");
const warningTypes = Object.keys(warnings_1.enabledWarnings);
function choiceWithDesc(choices, desc) {
    return {
        choices: Object.keys(choices),
        desc: [desc, ...Object.entries(choices).map(([choice, docs]) => `${choice}: ${docs}`)].join('\n'),
    };
}
var OPTION_GROUP;
(function (OPTION_GROUP) {
    OPTION_GROUP["JSII"] = "jsii compiler options:";
    OPTION_GROUP["TS"] = "TypeScript config options:";
})(OPTION_GROUP || (OPTION_GROUP = {}));
const ruleSets = {
    [tsconfig_1.TypeScriptConfigValidationRuleSet.STRICT]: 'Validates the provided config against a strict rule set designed for maximum backwards-compatibility.',
    [tsconfig_1.TypeScriptConfigValidationRuleSet.GENERATED]: 'Enforces a config as created by --generate-tsconfig. Use this to stay compatible with the generated config, but have full ownership over the file.',
    [tsconfig_1.TypeScriptConfigValidationRuleSet.MINIMAL]: 'Only enforce options that are known to be incompatible with jsii. This rule set is likely to be incomplete and new rules will be added without notice as incompatibilities emerge.',
    [tsconfig_1.TypeScriptConfigValidationRuleSet.NONE]: 'Disables all config validation, including options that are known to be incompatible with jsii. Intended for experimentation only. Use at your own risk.',
};
(async () => {
    await (0, support_1.emitSupportPolicyInformation)();
    await yargs
        .env('JSII')
        .command(['$0 [PROJECT_ROOT]', 'compile [PROJECT_ROOT]'], 'Compiles a jsii/TypeScript project', (argv) => argv
        .positional('PROJECT_ROOT', {
        type: 'string',
        desc: 'The root of the project to be compiled',
        default: '.',
        normalize: true,
    })
        .option('watch', {
        alias: 'w',
        type: 'boolean',
        desc: 'Watch for file changes and recompile automatically',
    })
        .option('project-references', {
        group: OPTION_GROUP.JSII,
        alias: 'r',
        type: 'boolean',
        desc: 'Generate TypeScript project references (also [package.json].jsii.projectReferences)\nHas no effect if --tsconfig is provided',
    })
        .option('fix-peer-dependencies', {
        type: 'boolean',
        default: true,
        desc: 'This option no longer has any effect.',
        hidden: true,
    })
        .options('fail-on-warnings', {
        group: OPTION_GROUP.JSII,
        alias: 'Werr',
        type: 'boolean',
        desc: 'Treat warnings as errors',
    })
        .option('silence-warnings', {
        group: OPTION_GROUP.JSII,
        type: 'array',
        default: [],
        desc: `List of warnings to silence (warnings: ${warningTypes.join(',')})`,
    })
        .option('strip-deprecated', {
        group: OPTION_GROUP.JSII,
        type: 'string',
        desc: '[EXPERIMENTAL] Hides all @deprecated members from the API (implementations remain). If an optional file name is given, only FQNs present in the file will be stripped.',
    })
        .option('add-deprecation-warnings', {
        group: OPTION_GROUP.JSII,
        type: 'boolean',
        default: false,
        desc: '[EXPERIMENTAL] Injects warning statements for all deprecated elements, to be printed at runtime',
    })
        .option('generate-tsconfig', {
        group: OPTION_GROUP.TS,
        type: 'string',
        defaultDescription: 'tsconfig.json',
        desc: 'Name of the typescript configuration file to generate with compiler settings',
    })
        .option('tsconfig', {
        group: OPTION_GROUP.TS,
        alias: 'c',
        type: 'string',
        desc: '[EXPERIMENTAL] Use this typescript configuration file to compile the jsii project.',
    })
        .conflicts('tsconfig', ['generate-tsconfig', 'project-references'])
        .option('validate-tsconfig', {
        group: OPTION_GROUP.TS,
        ...choiceWithDesc(ruleSets, '[EXPERIMENTAL] Validate the provided typescript configuration file against a set of rules.'),
        defaultDescription: tsconfig_1.TypeScriptConfigValidationRuleSet.STRICT,
    })
        .option('compress-assembly', {
        group: OPTION_GROUP.JSII,
        type: 'boolean',
        default: false,
        desc: 'Emit a compressed version of the assembly',
    })
        .option('verbose', {
        alias: 'v',
        type: 'count',
        desc: 'Increase the verbosity of output',
        global: true,
    }), async (argv) => {
        try {
            _configureLog4js(argv.verbose);
            if (argv['generate-tsconfig'] != null && argv.tsconfig != null) {
                throw new utils.JsiiError('Options --generate-tsconfig and --tsconfig are mutually exclusive', true);
            }
            const projectRoot = path.normalize(path.resolve(process.cwd(), argv.PROJECT_ROOT));
            const { projectInfo, diagnostics: projectInfoDiagnostics } = (0, project_info_1.loadProjectInfo)(projectRoot);
            // disable all silenced warnings
            for (const key of argv['silence-warnings']) {
                if (!(key in warnings_1.enabledWarnings)) {
                    throw new utils.JsiiError(`Unknown warning type ${key}. Must be one of: ${warningTypes.join(', ')}`);
                }
                warnings_1.enabledWarnings[key] = false;
            }
            (0, jsii_diagnostic_1.configureCategories)(projectInfo.diagnostics ?? {});
            const typeScriptConfig = argv.tsconfig ?? projectInfo.packageJson.jsii?.tsconfig;
            const validateTypeScriptConfig = argv['validate-tsconfig'] ??
                projectInfo.packageJson.jsii?.validateTsconfig ??
                tsconfig_1.TypeScriptConfigValidationRuleSet.STRICT;
            const compiler = new compiler_1.Compiler({
                projectInfo,
                projectReferences: argv['project-references'],
                failOnWarnings: argv['fail-on-warnings'],
                stripDeprecated: argv['strip-deprecated'] != null,
                stripDeprecatedAllowListFile: argv['strip-deprecated'],
                addDeprecationWarnings: argv['add-deprecation-warnings'],
                generateTypeScriptConfig: argv['generate-tsconfig'],
                typeScriptConfig,
                validateTypeScriptConfig,
                compressAssembly: argv['compress-assembly'],
            });
            const emitResult = argv.watch ? await compiler.watch() : compiler.emit();
            const allDiagnostics = [...projectInfoDiagnostics, ...emitResult.diagnostics];
            for (const diagnostic of allDiagnostics) {
                utils.logDiagnostic(diagnostic, projectRoot);
            }
            if (emitResult.emitSkipped) {
                process.exitCode = 1;
            }
        }
        catch (e) {
            if (e instanceof utils.JsiiError) {
                if (e.showHelp) {
                    console.log();
                    yargs.showHelp();
                    console.log();
                }
                const LOG = log4js.getLogger(utils.CLI_LOGGER);
                LOG.error(e.message);
                process.exitCode = -1;
            }
            else {
                throw e;
            }
        }
    })
        .help()
        .version(`${version_1.VERSION}, typescript ${package_json_1.version}`)
        .parse();
})().catch((e) => {
    console.error(`Error: ${e.stack}`);
    process.exitCode = -1;
});
function _configureLog4js(verbosity) {
    const stderrColor = !!process.stderr.isTTY;
    const stdoutColor = !!process.stdout.isTTY;
    log4js.addLayout('passThroughNoColor', () => {
        return (loggingEvent) => utils.stripAnsi(util.format(...loggingEvent.data));
    });
    log4js.configure({
        appenders: {
            console: {
                type: 'stderr',
                layout: { type: stderrColor ? 'colored' : 'basic' },
            },
            [utils.DIAGNOSTICS]: {
                type: 'stdout',
                layout: {
                    type: stdoutColor ? 'messagePassThrough' : 'passThroughNoColor',
                },
            },
            [utils.CLI_LOGGER]: {
                type: 'stderr',
                layout: {
                    type: 'pattern',
                    pattern: stdoutColor ? '%[[%p]%] %m' : '[%p] %m',
                },
            },
        },
        categories: {
            default: { appenders: ['console'], level: _logLevel() },
            [utils.CLI_LOGGER]: {
                appenders: [utils.CLI_LOGGER],
                level: _logLevel(),
            },
            // The diagnostics logger must be set to INFO or more verbose, or watch won't show important messages
            [utils.DIAGNOSTICS]: {
                appenders: [utils.DIAGNOSTICS],
                level: _logLevel(Math.max(verbosity, 1)),
            },
        },
    });
    function _logLevel(verbosityLevel = verbosity) {
        switch (verbosityLevel) {
            case 0:
                return 'WARN';
            case 1:
                return 'INFO';
            case 2:
                return 'DEBUG';
            case 3:
                return 'TRACE';
            default:
                return 'ALL';
        }
    }
}
//# sourceMappingURL=main.js.map