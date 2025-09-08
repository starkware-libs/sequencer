import { Construct } from 'constructs';
import { Include } from './include';
/**
 * Options for `Helm`.
 */
export interface HelmProps {
    /**
     * The chart name to use. It can be a chart from a helm repository or a local directory.
     *
     * This name is passed to `helm template` and has all the relevant semantics.
     *
     * @example "./mysql"
     * @example "bitnami/redis"
     */
    readonly chart: string;
    /**
     * Chart repository url where to locate the requested chart
     */
    readonly repo?: string;
    /**
     * Version constraint for the chart version to use.
     * This constraint can be a specific tag (e.g. 1.1.1)
     * or it may reference a valid range (e.g. ^2.0.0).
     * If this is not specified, the latest version is used
     *
     * This name is passed to `helm template --version` and has all the relevant semantics.
     *
     * @example "1.1.1"
     * @example "^2.0.0"
     */
    readonly version?: string;
    /**
     * Scope all resources in to a given namespace.
     */
    readonly namespace?: string;
    /**
     * The release name.
     *
     * @see https://helm.sh/docs/intro/using_helm/#three-big-concepts
     * @default - if unspecified, a name will be allocated based on the construct path
     */
    readonly releaseName?: string;
    /**
     * Values to pass to the chart.
     *
     * @default - If no values are specified, chart will use the defaults.
     */
    readonly values?: {
        [key: string]: any;
    };
    /**
     * The local helm executable to use in order to create the manifest the chart.
     *
     * @default "helm"
     */
    readonly helmExecutable?: string;
    /**
     * Additional flags to add to the `helm` execution.
     *
     * @default []
     */
    readonly helmFlags?: string[];
}
/**
 * Represents a Helm deployment.
 *
 * Use this construct to import an existing Helm chart and incorporate it into your constructs.
 */
export declare class Helm extends Include {
    /**
     * The helm release name.
     */
    readonly releaseName: string;
    constructor(scope: Construct, id: string, props: HelmProps);
}
