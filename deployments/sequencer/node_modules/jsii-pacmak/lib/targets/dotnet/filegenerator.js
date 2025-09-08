"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.FileGenerator = exports.DotNetDependency = void 0;
const path = require("path");
const xmlbuilder = require("xmlbuilder");
const __1 = require("..");
const logging = require("../../logging");
const version_1 = require("../../version");
const dotnet_1 = require("../dotnet");
const version_utils_1 = require("../version-utils");
const nameutils_1 = require("./nameutils");
// Represents a dependency in the dependency tree.
class DotNetDependency {
    constructor(namespace, packageId, fqn, version, partOfCompilation) {
        this.namespace = namespace;
        this.packageId = packageId;
        this.fqn = fqn;
        this.partOfCompilation = partOfCompilation;
        this.version = (0, version_utils_1.toNuGetVersionRange)(version);
    }
}
exports.DotNetDependency = DotNetDependency;
// Generates misc files such as the .csproj and the AssemblyInfo.cs file
// Uses the same instance of CodeMaker as the rest of the code so that the files get created when calling the save() method
class FileGenerator {
    // We pass in an instance of CodeMaker so that the files get later saved
    // when calling the save() method on the .NET Generator.
    constructor(assm, tarballFileName, code) {
        this.assemblyInfoNamespaces = [
            'Amazon.JSII.Runtime.Deputy',
        ];
        this.nameutils = new nameutils_1.DotNetNameUtils();
        this.assm = assm;
        this.tarballFileName = tarballFileName;
        this.code = code;
    }
    // Generates the .csproj file
    generateProjectFile(dependencies, iconFile) {
        const assembly = this.assm;
        const packageId = assembly.targets.dotnet.packageId;
        const projectFilePath = path.join(packageId, `${packageId}.csproj`);
        // Construct XML csproj content.
        // headless removes the <xml?> head node so that the first node is the <Project> node
        const rootNode = xmlbuilder.create('Project', {
            encoding: 'UTF-8',
            headless: true,
        });
        rootNode.att('Sdk', 'Microsoft.NET.Sdk');
        const propertyGroup = rootNode.ele('PropertyGroup');
        const dotnetInfo = assembly.targets.dotnet;
        propertyGroup.comment('Package Identification');
        propertyGroup.ele('Description', this.getDescription());
        if (iconFile != null) {
            propertyGroup.ele('PackageIcon', iconFile.split(/[/\\]+/).join('\\'));
            // We also need to actually include the icon in the package
            const noneNode = rootNode.ele('ItemGroup').ele('None');
            noneNode.att('Include', iconFile.split(/[/\\]+/).join('\\'));
            noneNode.att('Pack', 'true');
            noneNode.att('PackagePath', `\\${path
                .dirname(iconFile)
                .split(/[/\\]+/)
                .join('\\')}`);
        }
        // We continue to include the PackageIconUrl even if we put PackageIcon for backwards compatibility, as suggested
        // by https://docs.microsoft.com/en-us/nuget/reference/msbuild-targets#packageicon
        if (dotnetInfo.iconUrl != null) {
            propertyGroup.ele('PackageIconUrl', dotnetInfo.iconUrl);
        }
        propertyGroup.ele('PackageId', packageId);
        propertyGroup.ele('PackageLicenseExpression', assembly.license);
        propertyGroup.ele('PackageVersion', this.getDecoratedVersion(assembly));
        if (dotnetInfo.title != null) {
            propertyGroup.ele('Title', dotnetInfo.title);
        }
        propertyGroup.comment('Additional Metadata');
        propertyGroup.ele('Authors', assembly.author.name);
        if (assembly.author.organization) {
            propertyGroup.ele('Company', assembly.author.name);
        }
        if (assembly.keywords) {
            propertyGroup.ele('PackageTags', assembly.keywords.join(';'));
        }
        propertyGroup.ele('Language', 'en-US');
        propertyGroup.ele('ProjectUrl', assembly.homepage);
        propertyGroup.ele('RepositoryUrl', assembly.repository.url);
        propertyGroup.ele('RepositoryType', assembly.repository.type);
        propertyGroup.comment('Build Configuration');
        propertyGroup.ele('GenerateDocumentationFile', 'true');
        propertyGroup.ele('GeneratePackageOnBuild', 'true');
        propertyGroup.ele('IncludeSymbols', 'true');
        propertyGroup.ele('IncludeSource', 'true');
        propertyGroup.ele('Nullable', 'enable');
        propertyGroup.ele('SymbolPackageFormat', 'snupkg');
        propertyGroup.ele('TargetFramework', dotnet_1.TARGET_FRAMEWORK);
        // Transparently roll forward across major SDK releases if needed
        propertyGroup.ele('RollForward', 'Major');
        const itemGroup1 = rootNode.ele('ItemGroup');
        const embeddedResource = itemGroup1.ele('EmbeddedResource');
        embeddedResource.att('Include', this.tarballFileName);
        const itemGroup2 = rootNode.ele('ItemGroup');
        const packageReference = itemGroup2.ele('PackageReference');
        packageReference.att('Include', 'Amazon.JSII.Runtime');
        packageReference.att('Version', (0, version_utils_1.toNuGetVersionRange)(`^${version_1.VERSION}`));
        dependencies.forEach((value) => {
            if (value.partOfCompilation) {
                const dependencyReference = itemGroup2.ele('ProjectReference');
                dependencyReference.att('Include', `../${value.packageId}/${value.packageId}.csproj`);
            }
            else {
                const dependencyReference = itemGroup2.ele('PackageReference');
                dependencyReference.att('Include', value.packageId);
                dependencyReference.att('Version', value.version);
            }
        });
        const warnings = rootNode.ele('PropertyGroup');
        // Suppress warnings about [Obsolete] members, this is the author's choice!
        warnings.comment('Silence [Obsolete] warnings');
        warnings.ele('NoWarn').text('0612,0618');
        // Treat select warnings as errors, as these are likely codegen bugs:
        warnings.comment('Treat warnings symptomatic of code generation bugs as errors');
        warnings.ele('WarningsAsErrors', [
            '0108', // 'member1' hides inherited member 'member2'. Use the new keyword if hiding was intended.
            '0109', // The member 'member' does not hide an inherited member. The new keyword is not required.
        ].join(','));
        const xml = rootNode.end({ pretty: true, spaceBeforeSlash: true });
        // Sending the xml content to the codemaker to ensure the file is written
        // and added to the file list for tracking
        this.code.openFile(projectFilePath);
        this.code.open(xml);
        // Unindent for the next file
        this.code.close();
        this.code.closeFile(projectFilePath);
        logging.debug(`Written to ${projectFilePath}`);
    }
    // Generates the AssemblyInfo.cs file
    generateAssemblyInfoFile() {
        const packageId = this.assm.targets.dotnet.packageId;
        const filePath = path.join(packageId, 'AssemblyInfo.cs');
        this.code.openFile(filePath);
        this.assemblyInfoNamespaces.map((n) => this.code.line(`using ${n};`));
        this.code.line();
        const assembly = `[assembly: JsiiAssembly("${this.assm.name}", "${this.assm.version}", "${this.tarballFileName}")]`;
        this.code.line(assembly);
        this.code.closeFile(filePath);
    }
    // Generates the description
    getDescription() {
        const docs = this.assm.docs;
        if (docs) {
            const stability = docs.stability;
            if (stability) {
                return `${this.assm.description} (Stability: ${this.nameutils.capitalizeWord(stability)})`;
            }
        }
        return this.assm.description;
    }
    // Generates the decorated version
    getDecoratedVersion(assembly) {
        const suffix = assembly.targets.dotnet.versionSuffix;
        if (suffix) {
            // suffix is guaranteed to start with a leading `-`
            return `${assembly.version}${suffix}`;
        }
        return (0, version_utils_1.toReleaseVersion)(assembly.version, __1.TargetName.DOTNET);
    }
}
exports.FileGenerator = FileGenerator;
//# sourceMappingURL=filegenerator.js.map