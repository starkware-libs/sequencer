import * as spec from '@jsii/spec';
import * as ts from 'typescript';
export declare function setRelatedNode<T, N extends ts.Node = ts.Node>(object: T, node: N | undefined): T;
export declare const setClassRelatedNode: (object: spec.ClassType, node: ts.ClassDeclaration | undefined) => spec.ClassType;
export declare const setEnumRelatedNode: (object: spec.EnumType, node: ts.EnumDeclaration | undefined) => spec.EnumType;
export declare const setInterfaceRelatedNode: (object: spec.InterfaceType, node: ts.InterfaceDeclaration | undefined) => spec.InterfaceType;
export declare const setMethodRelatedNode: <T extends ts.MethodDeclaration | ts.MethodSignature>(object: spec.Method, node: T | undefined) => spec.Method;
export declare const setParameterRelatedNode: (object: spec.Parameter, node: ts.ParameterDeclaration | undefined) => spec.Parameter;
export declare const setPropertyRelatedNode: (object: spec.Property, node: ts.AccessorDeclaration | ts.ParameterPropertyDeclaration | ts.PropertyDeclaration | ts.PropertySignature | undefined) => spec.Parameter;
export declare function getRelatedNode<T extends ts.Node = ts.Node>(object: any): T | undefined;
export declare const getClassRelatedNode: (object: spec.ClassType) => ts.ClassDeclaration | undefined;
export declare const getClassOrInterfaceRelatedNode: (object: spec.ClassType | spec.InterfaceType) => ts.ClassDeclaration | ts.InterfaceDeclaration | undefined;
export declare const getEnumRelatedNode: (object: spec.EnumType) => ts.EnumDeclaration | undefined;
export declare const getInterfaceRelatedNode: (object: spec.InterfaceType) => ts.InterfaceDeclaration | undefined;
export declare const getMethodRelatedNode: (object: spec.Method) => ts.MethodDeclaration | ts.MethodSignature | undefined;
export declare const getParameterRelatedNode: (object: spec.Parameter) => ts.AccessorDeclaration | ts.ParameterPropertyDeclaration | ts.PropertyDeclaration | ts.PropertySignature | undefined;
export declare const getPropertyRelatedNode: (object: spec.Parameter) => ts.AccessorDeclaration | ts.ParameterPropertyDeclaration | ts.PropertyDeclaration | ts.PropertySignature | undefined;
export declare const getTypeRelatedNode: (object: spec.Type) => ts.ClassDeclaration | ts.EnumDeclaration | ts.InterfaceDeclaration | undefined;
//# sourceMappingURL=node-bindings.d.ts.map