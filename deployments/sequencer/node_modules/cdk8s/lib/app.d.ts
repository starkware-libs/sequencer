import { Construct, IConstruct } from 'constructs';
import { Chart } from './chart';
import { IResolver } from './resolve';
/** The method to divide YAML output into files */
export declare enum YamlOutputType {
    /** All resources are output into a single YAML file */
    FILE_PER_APP = 0,
    /** Resources are split into seperate files by chart */
    FILE_PER_CHART = 1,
    /** Each resource is output to its own file */
    FILE_PER_RESOURCE = 2,
    /** Each chart in its own folder and each resource in its own file */
    FOLDER_PER_CHART_FILE_PER_RESOURCE = 3
}
export interface AppProps {
    /**
     * The directory to output Kubernetes manifests.
     *
     * If you synthesize your application using `cdk8s synth`, you must
     * also pass this value to the CLI using the `--output` option or
     * the `output` property in the `cdk8s.yaml` configuration file.
     * Otherwise, the CLI will not know about the output directory,
     * and synthesis will fail.
     *
     * This property is intended for internal and testing use.
     *
     * @default - CDK8S_OUTDIR if defined, otherwise "dist"
     */
    readonly outdir?: string;
    /**
     *  The file extension to use for rendered YAML files
     * @default .k8s.yaml
     */
    readonly outputFileExtension?: string;
    /**
     *  How to divide the YAML output into files
     * @default YamlOutputType.FILE_PER_CHART
     */
    readonly yamlOutputType?: YamlOutputType;
    /**
     * When set to true, the output directory will contain a `construct-metadata.json` file
     * that holds construct related metadata on every resource in the app.
     *
     * @default false
     */
    readonly recordConstructMetadata?: boolean;
    /**
     * A list of resolvers that can be used to replace property values before
     * they are written to the manifest file. When multiple resolvers are passed,
     * they are invoked by order in the list, and only the first one that applies
     * (e.g calls `context.replaceValue`) is invoked.
     *
     * @see https://cdk8s.io/docs/latest/basics/app/#resolvers
     *
     * @default - no resolvers.
     */
    readonly resolvers?: IResolver[];
}
/**
 * Represents a cdk8s application.
 */
export declare class App extends Construct {
    /**
     * Synthesize a single chart.
     *
     * Each element returned in the resulting array represents a different ApiObject
     * in the scope of the chart.
     *
     * Note that the returned array order is important. It is determined by the various dependencies between
     * the constructs in the chart, where the first element is the one without dependencies, and so on...
     *
     * @returns An array of JSON objects.
     * @param chart the chart to synthesize.
     * @internal
     */
    static _synthChart(chart: Chart): any[];
    static of(c: IConstruct): App;
    /**
     * The output directory into which manifests will be synthesized.
     */
    readonly outdir: string;
    /**
     *  The file extension to use for rendered YAML files
     * @default .k8s.yaml
     */
    readonly outputFileExtension: string;
    /** How to divide the YAML output into files
     * @default YamlOutputType.FILE_PER_CHART
     */
    readonly yamlOutputType: YamlOutputType;
    /**
     * Resolvers used by this app. This includes both custom resolvers
     * passed by the `resolvers` property, as well as built-in resolvers.
     */
    readonly resolvers: IResolver[];
    private readonly recordConstructMetadata;
    /**
     * Returns all the charts in this app, sorted topologically.
     */
    get charts(): Chart[];
    /**
     * Defines an app
     * @param props configuration options
     */
    constructor(props?: AppProps);
    /**
     * Synthesizes all manifests to the output directory
     */
    synth(): void;
    /**
     * Synthesizes the app into a YAML string.
     *
     * @returns A string with all YAML objects across all charts in this app.
     */
    synthYaml(): string;
    private writeConstructMetadata;
}
