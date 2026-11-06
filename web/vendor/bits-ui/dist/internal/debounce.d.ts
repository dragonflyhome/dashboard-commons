export declare function debounce<T extends (...args: any[]) => any>(fn: T, wait?: number): {
    (...args: Parameters<T>): void;
    flush(): void;
    destroy(): void;
};
