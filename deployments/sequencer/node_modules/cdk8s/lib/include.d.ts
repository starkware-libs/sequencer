import { Construct } from 'constructs';
import { ApiObject } from './api-object';
export interface IncludeProps {
    /**
     * Local file path or URL which includes a Kubernetes YAML manifest.
     *
     * @example mymanifest.yaml
     */
    readonly url: string;
}
/**
 * Reads a YAML manifest from a file or a URL and defines all resources as API
 * objects within the defined scope.
 *
 * The names (`metadata.name`) of imported resources will be preserved as-is
 * from the manifest.
 */
export declare class Include extends Construct {
    constructor(scope: Construct, id: string, props: IncludeProps);
    /**
     * Returns all the included API objects.
     */
    get apiObjects(): ApiObject[];
}
