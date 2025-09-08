import * as spec from '@jsii/spec';
import { CodeMaker } from 'codemaker';
import { DotNetTypeResolver } from './dotnettyperesolver';
/**
 * Generates the Jsii attributes and calls for the jsii .NET runtime
 *
 * Uses the same instance of CodeMaker as the rest of the code
 */
export declare class DotNetRuntimeGenerator {
    private readonly code;
    private readonly typeresolver;
    private readonly nameutils;
    constructor(code: CodeMaker, typeresolver: DotNetTypeResolver);
    /**
     * Emits the jsii attribute for an interface
     *
     * Ex: [JsiiInterface(nativeType: typeof(IGreetee), fullyQualifiedName: "jsii-calc.Greetee")]
     */
    emitAttributesForInterface(ifc: spec.InterfaceType): void;
    /**
     * Emits the jsii attribute for an interface datatype
     *
     * @param ifc the annotated interface type.
     *
     * Ex: [JsiiByValue(fqn: "assembly.TypeName")]
     */
    emitAttributesForInterfaceDatatype(ifc: spec.InterfaceType): void;
    /**
     * Emits the jsii attribute for a class
     *
     * Ex: [JsiiClass(nativeType: typeof(Very), fullyQualifiedName: "@scope/jsii-calc-base-of-base.Very")]
     */
    emitAttributesForClass(cls: spec.ClassType): void;
    /**
     * Emits the proper jsii .NET attribute for a method
     *
     * Ex: [JsiiMethod(name: "hey", returnsJson: "{\"type\":{\"primitive\":\"number\"}}")
     */
    emitAttributesForMethod(cls: spec.ClassType | spec.InterfaceType, method: spec.Method): void;
    /**
     * Emits the proper jsii .NET attribute for a property
     *
     * Ex: [JsiiProperty(name: "foo", typeJson: "{\"fqn\":\"@scope/jsii-calc-base-of-base.Very\"}", isOptional: true)]
     */
    emitAttributesForProperty(prop: spec.Property): void;
    /**
     * Emits the proper jsii .NET attribute for an interface proxy
     *
     * Ex: [JsiiTypeProxy(nativeType: typeof(IVeryBaseProps), fullyQualifiedName: "@scope/jsii-calc-base-of-base.VeryBaseProps")]
     */
    emitAttributesForInterfaceProxy(ifc: spec.ClassType | spec.InterfaceType): void;
    /**
     * Emits the proper jsii .NET attribute for an enum
     *
     * Ex: [JsiiEnum(nativeType: typeof(Test), fullyQualifiedName: "jsii-calc.Test")]
     */
    emitAttributesForEnum(enm: spec.EnumType, enumName: string): void;
    /**
     * Emits the proper jsii .NET attribute for an enum member
     *
     * Ex: [JsiiEnumMember(name: "Normal")]
     */
    emitAttributesForEnumMember(enumMemberName: string, enmmember: spec.EnumMember): void;
    /**
     * Returns the jsii .NET method identifier
     */
    createInvokeMethodIdentifier(method: spec.Method, cls: spec.ClassType): string;
    /**
     * Emits the proper .NET attribute for a deprecated class/interface/member
     *
     * Ex: [System.Obsolete()]
     */
    emitDeprecatedAttributeIfNecessary(obj: spec.Method | spec.ClassType | spec.InterfaceType | spec.Property | spec.EnumType | spec.EnumMember | spec.Initializer | undefined): void;
}
//# sourceMappingURL=dotnetruntimegenerator.d.ts.map