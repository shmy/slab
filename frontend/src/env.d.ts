/// <reference types="@rsbuild/core/types" />

interface ImportMeta {
    readonly brandName: string;
}

/** history@4 未自带类型声明 */
declare module 'history';

declare var amisScoped: {
    doAction: (action: any) => void;
};
declare var PetiteVue: any;
declare var amisRequire: (name: string) => any;
declare var _j: (schema: Record<string, unknown>) => void;
