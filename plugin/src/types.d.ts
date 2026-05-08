declare module "*.wasm" {
	const binary: Uint8Array;
	export default binary;
}

declare module "sql.js" {
	export class Database {
		constructor(data?: ArrayLike<number> | ArrayBuffer);
		exec(sql: string, params?: unknown): unknown;
		close(): void;
		prepare(sql: string): Statement;
	}
	export class Statement {
		bind(values: unknown[] | Record<string, unknown>): void;
		step(): boolean;
		getAsObject(): Record<string, unknown>;
		free(): void;
	}
	function initSqlJs(
		config?: {
			locateFile?: (file: string) => string;
			wasmBinary?: Uint8Array;
		},
	): Promise<{
		Database: typeof Database;
	}>;
	export default initSqlJs;
}
