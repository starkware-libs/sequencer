export declare class Lazy {
    private readonly producer;
    static any(producer: IAnyProducer): any;
    private constructor();
    produce(): any;
}
export interface IAnyProducer {
    produce(): any;
}
