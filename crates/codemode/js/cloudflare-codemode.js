// Adapted from @cloudflare/codemode 0.5.6 (https://github.com/cloudflare/agents, packages/codemode,
// commit 4116a8bdf97660c8e79a923b6aec140256282a5d), MIT License, Copyright (c) 2025 Cloudflare, Inc.
// (see LICENSE-cloudflare). This is the package's published JavaScript (dist/json-schema-types-*.js,
// dist/truncate-*.js, and DEFAULT_DESCRIPTION from dist/shared-*.js), modified for QuickJS:
// - no ES module syntax: `acorn` is the global that acorn.js defines, and nothing is exported;
// - Number.prototype.toLocaleString() is replaced by __enUS(n): QuickJS formats numbers without
//   grouping, and Workers (V8, en-US) write 6,000.
// Everything else is as published.

/** n formatted as Number.prototype.toLocaleString() formats it in en-US (V8): 6000 -> "6,000". */
function __enUS(n) {
	if (typeof n !== "number" || !Number.isFinite(n) || !Number.isInteger(n)) return String(n);
	const s = String(Math.abs(n));
	let out = "";
	for (let i = 0; i < s.length; i++) {
		if (i > 0 && (s.length - i) % 3 === 0) out += ",";
		out += s[i];
	}
	return (n < 0 ? "-" : "") + out;
}

//#region src/normalize.ts
/**
* Strip markdown code fences that LLMs commonly wrap code in.
* Handles ```js, ```javascript, ```typescript, ```ts, or bare ```.
*/
function stripCodeFences(code) {
	const match = code.match(/^```(?:js|javascript|typescript|ts|tsx|jsx)?\s*\n([\s\S]*?)```\s*$/);
	return match ? match[1] : code;
}
function normalizeCode(code) {
	const trimmed = stripCodeFences(code.trim());
	if (!trimmed.trim()) return "async () => {}";
	const source = trimmed.trim();
	try {
		const ast = acorn.parse(source, {
			ecmaVersion: "latest",
			sourceType: "module"
		});
		if (ast.body.length === 1 && ast.body[0].type === "ExpressionStatement") {
			if (ast.body[0].expression.type === "ArrowFunctionExpression") return source;
		}
		if (ast.body.length === 1 && ast.body[0].type === "ExportDefaultDeclaration") {
			const decl = ast.body[0].declaration;
			const inner = source.slice(decl.start, decl.end);
			if (decl.type === "FunctionDeclaration" && !decl.id) return `async () => {\nreturn (${inner})();\n}`;
			if (decl.type === "ClassDeclaration" && !decl.id) return `async () => {\nreturn (${inner});\n}`;
			return normalizeCode(inner);
		}
		if (ast.body.length === 1 && ast.body[0].type === "FunctionDeclaration") return `async () => {\n${source}\nreturn ${ast.body[0].id?.name ?? "fn"}();\n}`;
		const last = ast.body[ast.body.length - 1];
		if (last?.type === "ExpressionStatement") {
			const exprStmt = last;
			return `async () => {\n${source.slice(0, last.start)}return (${source.slice(exprStmt.expression.start, exprStmt.expression.end)})\n}`;
		}
		return `async () => {\n${source}\n}`;
	} catch {
		return `async () => {\n${source}\n}`;
	}
}
//#endregion
//#region src/utils.ts
const JS_RESERVED = /* @__PURE__ */ new Set([
	"abstract",
	"arguments",
	"await",
	"boolean",
	"break",
	"byte",
	"case",
	"catch",
	"char",
	"class",
	"const",
	"continue",
	"debugger",
	"default",
	"delete",
	"do",
	"double",
	"else",
	"enum",
	"eval",
	"export",
	"extends",
	"false",
	"final",
	"finally",
	"float",
	"for",
	"function",
	"goto",
	"if",
	"implements",
	"import",
	"in",
	"instanceof",
	"int",
	"interface",
	"let",
	"long",
	"native",
	"new",
	"null",
	"package",
	"private",
	"protected",
	"public",
	"return",
	"short",
	"static",
	"super",
	"switch",
	"synchronized",
	"this",
	"throw",
	"throws",
	"transient",
	"true",
	"try",
	"typeof",
	"undefined",
	"var",
	"void",
	"volatile",
	"while",
	"with",
	"yield"
]);
/**
* Sanitize a tool name into a valid JavaScript identifier.
* Replaces hyphens, dots, and spaces with `_`, strips other invalid chars,
* prefixes digit-leading names with `_`, and appends `_` to JS reserved words.
*/
function sanitizeToolName(name) {
	if (!name) return "_";
	let sanitized = name.replace(/[-.\s]/g, "_");
	sanitized = sanitized.replace(/[^a-zA-Z0-9_$]/g, "");
	if (!sanitized) return "_";
	if (/^[0-9]/.test(sanitized)) sanitized = "_" + sanitized;
	if (JS_RESERVED.has(sanitized)) sanitized = sanitized + "_";
	return sanitized;
}
function toPascalCase(str) {
	return str.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase()).replace(/^[a-z]/, (letter) => letter.toUpperCase());
}
/**
* Escape a character as a unicode escape sequence if it is a control character.
*/
function escapeControlChar(ch) {
	const code = ch.charCodeAt(0);
	if (code <= 31 || code === 127) return "\\u" + code.toString(16).padStart(4, "0");
	return ch;
}
/**
* Quote a property name if needed.
* Escapes backslashes, quotes, and control characters.
*/
function quoteProp(name) {
	if (!/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name)) {
		let escaped = "";
		for (const ch of name) if (ch === "\\") escaped += "\\\\";
		else if (ch === "\"") escaped += "\\\"";
		else if (ch === "\n") escaped += "\\n";
		else if (ch === "\r") escaped += "\\r";
		else if (ch === "	") escaped += "\\t";
		else if (ch === "\u2028") escaped += "\\u2028";
		else if (ch === "\u2029") escaped += "\\u2029";
		else escaped += escapeControlChar(ch);
		return `"${escaped}"`;
	}
	return name;
}
/**
* Escape a string for use inside a double-quoted TypeScript string literal.
*/
function escapeStringLiteral(s) {
	let out = "";
	for (const ch of s) if (ch === "\\") out += "\\\\";
	else if (ch === "\"") out += "\\\"";
	else if (ch === "\n") out += "\\n";
	else if (ch === "\r") out += "\\r";
	else if (ch === "	") out += "\\t";
	else if (ch === "\u2028") out += "\\u2028";
	else if (ch === "\u2029") out += "\\u2029";
	else out += escapeControlChar(ch);
	return out;
}
/**
* Escape a string for use inside a JSDoc comment.
* Prevents premature comment closure from star-slash sequences.
*/
function escapeJsDoc(text) {
	return text.replace(/\*\//g, "*\\/");
}
//#endregion
//#region src/json-schema-types.ts
/**
* Resolve an internal JSON Pointer $ref (e.g. #/definitions/Foo) against the root schema.
* Returns null for external URLs or unresolvable paths.
*/
function resolveRef(ref, root) {
	if (ref === "#") return root;
	if (!ref.startsWith("#/")) return null;
	const segments = ref.slice(2).split("/").map((s) => s.replace(/~1/g, "/").replace(/~0/g, "~"));
	let current = root;
	for (const seg of segments) {
		if (current === null || typeof current !== "object") return null;
		current = current[seg];
		if (current === void 0) return null;
	}
	if (typeof current === "boolean") return current;
	if (current === null || typeof current !== "object") return null;
	return current;
}
/**
* Apply OpenAPI 3.0 `nullable: true` to a type result.
*/
function applyNullable(result, schema) {
	if (result !== "unknown" && result !== "never" && schema?.nullable === true) return `${result} | null`;
	return result;
}
/**
* Convert a JSON Schema to a TypeScript type string.
* This is a direct conversion without going through Zod.
*/
function jsonSchemaToTypeString(schema, indent, ctx) {
	if (typeof schema === "boolean") return schema ? "unknown" : "never";
	if (ctx.depth >= ctx.maxDepth) return "unknown";
	if (ctx.seen.has(schema)) return "unknown";
	ctx.seen.add(schema);
	const nextCtx = {
		...ctx,
		depth: ctx.depth + 1
	};
	try {
		if (schema.$ref) {
			const resolved = resolveRef(schema.$ref, ctx.root);
			if (!resolved) return "unknown";
			return applyNullable(jsonSchemaToTypeString(resolved, indent, nextCtx), schema);
		}
		if (schema.anyOf) return applyNullable(schema.anyOf.map((s) => jsonSchemaToTypeString(s, indent, nextCtx)).join(" | "), schema);
		if (schema.oneOf) return applyNullable(schema.oneOf.map((s) => jsonSchemaToTypeString(s, indent, nextCtx)).join(" | "), schema);
		if (schema.allOf) return applyNullable(schema.allOf.map((s) => jsonSchemaToTypeString(s, indent, nextCtx)).join(" & "), schema);
		if (schema.enum) {
			if (schema.enum.length === 0) return "never";
			return applyNullable(schema.enum.map((v) => {
				if (v === null) return "null";
				if (typeof v === "string") return "\"" + escapeStringLiteral(v) + "\"";
				if (typeof v === "object") return JSON.stringify(v) ?? "unknown";
				return String(v);
			}).join(" | "), schema);
		}
		if (schema.const !== void 0) return applyNullable(schema.const === null ? "null" : typeof schema.const === "string" ? "\"" + escapeStringLiteral(schema.const) + "\"" : typeof schema.const === "object" ? JSON.stringify(schema.const) ?? "unknown" : String(schema.const), schema);
		const type = schema.type;
		if (type === "string") return applyNullable("string", schema);
		if (type === "number" || type === "integer") return applyNullable("number", schema);
		if (type === "boolean") return applyNullable("boolean", schema);
		if (type === "null") return "null";
		if (type === "array") {
			const prefixItems = schema.prefixItems;
			if (Array.isArray(prefixItems)) return applyNullable(`[${prefixItems.map((s) => jsonSchemaToTypeString(s, indent, nextCtx)).join(", ")}]`, schema);
			if (Array.isArray(schema.items)) return applyNullable(`[${schema.items.map((s) => jsonSchemaToTypeString(s, indent, nextCtx)).join(", ")}]`, schema);
			if (schema.items) return applyNullable(`${jsonSchemaToTypeString(schema.items, indent, nextCtx)}[]`, schema);
			return applyNullable("unknown[]", schema);
		}
		if (type === "object" || schema.properties) {
			const props = schema.properties || {};
			const required = new Set(schema.required || []);
			const lines = [];
			for (const [propName, propSchema] of Object.entries(props)) {
				if (typeof propSchema === "boolean") {
					const boolType = propSchema ? "unknown" : "never";
					const optionalMark = required.has(propName) ? "" : "?";
					lines.push(`${indent}    ${quoteProp(propName)}${optionalMark}: ${boolType};`);
					continue;
				}
				const isRequired = required.has(propName);
				const propType = jsonSchemaToTypeString(propSchema, indent + "    ", nextCtx);
				const desc = propSchema.description;
				const format = propSchema.format;
				if (desc || format) {
					const descText = desc ? escapeJsDoc(desc.replace(/\r?\n/g, " ")) : void 0;
					const formatTag = format ? `@format ${escapeJsDoc(format)}` : void 0;
					if (descText && formatTag) {
						lines.push(`${indent}    /**`);
						lines.push(`${indent}     * ${descText}`);
						lines.push(`${indent}     * ${formatTag}`);
						lines.push(`${indent}     */`);
					} else lines.push(`${indent}    /** ${descText ?? formatTag} */`);
				}
				const quotedName = quoteProp(propName);
				const optionalMark = isRequired ? "" : "?";
				lines.push(`${indent}    ${quotedName}${optionalMark}: ${propType};`);
			}
			if (schema.additionalProperties) {
				const valueType = schema.additionalProperties === true ? "unknown" : jsonSchemaToTypeString(schema.additionalProperties, indent + "    ", nextCtx);
				lines.push(`${indent}    [key: string]: ${valueType};`);
			}
			if (lines.length === 0) {
				if (schema.additionalProperties === false) return applyNullable("{}", schema);
				return applyNullable("Record<string, unknown>", schema);
			}
			return applyNullable(`{\n${lines.join("\n")}\n${indent}}`, schema);
		}
		if (Array.isArray(type)) return applyNullable(type.map((t) => {
			if (t === "string") return "string";
			if (t === "number" || t === "integer") return "number";
			if (t === "boolean") return "boolean";
			if (t === "null") return "null";
			if (t === "array") return "unknown[]";
			if (t === "object") return "Record<string, unknown>";
			return "unknown";
		}).join(" | "), schema);
		return "unknown";
	} finally {
		ctx.seen.delete(schema);
	}
}
/**
* Convert a JSON Schema to a TypeScript type declaration.
*/
function jsonSchemaToType(schema, typeName) {
	return `type ${typeName} = ${jsonSchemaToTypeString(schema, "", {
		root: schema,
		depth: 0,
		seen: /* @__PURE__ */ new Set(),
		maxDepth: 20
	})}`;
}
/**
* Extract field descriptions from a JSON Schema's properties.
*/
function extractJsonSchemaDescriptions(schema) {
	const descriptions = {};
	if (schema.properties) {
		for (const [fieldName, propSchema] of Object.entries(schema.properties)) if (propSchema && typeof propSchema === "object" && propSchema.description) descriptions[fieldName] = propSchema.description;
	}
	return descriptions;
}
/**
* Generate TypeScript type definitions from tool descriptors with JSON Schema.
* This function has NO dependency on the AI SDK or Zod — it works purely with
* JSON Schema objects.
*
* Use this when you have raw JSON Schema (e.g. from OpenAPI specs, MCP tool
* definitions, etc.) and don't need the AI SDK.
*/
function generateTypesFromJsonSchema(tools) {
	let availableTools = "";
	let availableTypes = "";
	for (const [toolName, tool] of Object.entries(tools)) {
		const safeName = sanitizeToolName(toolName);
		const typeName = toPascalCase(safeName);
		try {
			const inputType = jsonSchemaToType(tool.inputSchema, `${typeName}Input`);
			const outputType = tool.outputSchema ? jsonSchemaToType(tool.outputSchema, `${typeName}Output`) : `type ${typeName}Output = unknown`;
			availableTypes += `\n${inputType.trim()}`;
			availableTypes += `\n${outputType.trim()}`;
			const paramLines = (() => {
				try {
					const paramDescs = extractJsonSchemaDescriptions(tool.inputSchema);
					return Object.entries(paramDescs).map(([fieldName, desc]) => `@param input.${fieldName} - ${desc}`);
				} catch {
					return [];
				}
			})();
			const jsdocLines = [];
			if (tool.description?.trim()) jsdocLines.push(escapeJsDoc(tool.description.trim().replace(/\r?\n/g, " ")));
			else jsdocLines.push(escapeJsDoc(toolName));
			for (const pd of paramLines) jsdocLines.push(escapeJsDoc(pd.replace(/\r?\n/g, " ")));
			const jsdocBody = jsdocLines.map((l) => `\t * ${l}`).join("\n");
			availableTools += `\n\t/**\n${jsdocBody}\n\t */`;
			availableTools += `\n\t${safeName}: (input: ${typeName}Input) => Promise<${typeName}Output>;`;
			availableTools += "\n";
		} catch {
			availableTypes += `\ntype ${typeName}Input = unknown`;
			availableTypes += `\ntype ${typeName}Output = unknown`;
			availableTools += `\n\t/**\n\t * ${escapeJsDoc(toolName)}\n\t */`;
			availableTools += `\n\t${safeName}: (input: ${typeName}Input) => Promise<${typeName}Output>;`;
			availableTools += "\n";
		}
	}
	availableTools = `\ndeclare const codemode: {${availableTools}}`;
	return `
${availableTypes}
${availableTools}
  `.trim();
}
//#endregion

//#region src/truncate.ts
/**
* Result truncation utilities.
*
* Tool and sandbox results can be large enough to blow a model's context
* window. These cap the serialized size of a value while leaving small,
* structured results intact so the model can still reason over them. They are
* the default building blocks for a `transformResult` hook (see
* `createCodemodeRuntime`).
*
* Structured values are truncated structurally: the output is always valid
* JSON of the same shape, with the largest values cut first. A truncated
* string ends with a marker, a truncated array ends with a marker element
* counting the dropped items, and an object that had to lose entries carries a
* marker entry naming the omitted keys. Every marker contains
* `--- TRUNCATED ---` so callers and models can find them.
*/
/** ~4 characters per token is a reasonable cross-model estimate. */
const CHARS_PER_TOKEN = 4;
const DEFAULT_MAX_TOKENS = 6e3;
const TRUNCATION_MARKER = "--- TRUNCATED ---";
/**
* The least a string or container is worth keeping at (see `floorSize`). A
* value that would get fewer serialized characters than this is dropped in
* favour of its siblings rather than reduced to a bare marker.
*/
const MIN_SLOT = 128;
function budget(options) {
	const maxTokens = options?.maxTokens ?? DEFAULT_MAX_TOKENS;
	return {
		maxChars: options?.maxChars ?? maxTokens * CHARS_PER_TOKEN,
		maxTokens
	};
}
/**
* Truncate a text response to a character budget, appending a marker that notes
* the original size so the model knows the output was clipped. Returns the
* input unchanged when it is within budget.
*/
function truncateResponse(text, options) {
	const { maxChars, maxTokens } = budget(options);
	if (text.length <= maxChars) return text;
	const estimatedTokens = Math.ceil(text.length / CHARS_PER_TOKEN);
	return text.slice(0, maxChars) + `\n\n${TRUNCATION_MARKER}\nResponse was ~${__enUS(estimatedTokens)} tokens (limit: ${__enUS(maxTokens)}). Narrow the request to reduce response size.`;
}
/**
* Truncate a structured result. Strings are truncated directly. Other values
* pass through unchanged (same reference) when their JSON serialization is
* within budget. When oversized, the value is shrunk structurally — largest
* values first, deepest last — so the model gets a bounded value that is still
* valid JSON of the original shape, with markers where content was cut.
*
* Values that can't be serialized (cycles, bigint, `undefined`) are returned
* unchanged.
*/
function truncateResult(value, options) {
	if (typeof value === "string") return truncateResponse(value, options);
	let serialized;
	try {
		serialized = JSON.stringify(value);
	} catch {
		return value;
	}
	if (serialized === void 0) return value;
	const { maxChars } = budget(options);
	if (serialized.length <= maxChars) return value;
	const shrunk = shrink(JSON.parse(serialized), maxChars);
	return size(shrunk) <= maxChars ? shrunk : truncateResponse(serialized, options);
}
/** Serialized size in characters (compact JSON). */
function size(value) {
	return JSON.stringify(value).length;
}
/** Only strings and containers can give up characters; scalars are atomic. */
function shrinkable(value) {
	return typeof value === "string" || typeof value === "object" && !!value;
}
/**
* The least budget a value is kept at; a scalar must fit whole. The floor
* never exceeds half the container's own budget, so tiny budgets still keep
* something rather than nothing.
*/
function floorSize(value, maxChars) {
	const full = size(value);
	return shrinkable(value) ? Math.min(full, MIN_SLOT, Math.max(2, Math.floor(maxChars / 2))) : full;
}
function shrink(value, maxChars) {
	if (size(value) <= maxChars) return value;
	if (typeof value === "string") return shrinkString(value, maxChars);
	if (Array.isArray(value)) return shrinkArray(value, maxChars);
	if (typeof value === "object" && value !== null) return shrinkObject(value, maxChars);
	return value;
}
function shrinkString(value, maxChars) {
	const suffix = ` ${TRUNCATION_MARKER} ${__enUS(value.length)} chars`;
	let keep = Math.max(0, maxChars - suffix.length - 2);
	let out = value.slice(0, keep) + suffix;
	while (keep > 0 && size(out) > maxChars) {
		keep = Math.max(0, keep - (size(out) - maxChars));
		out = value.slice(0, keep) + suffix;
	}
	return size(out) <= maxChars ? out : TRUNCATION_MARKER.slice(0, Math.max(0, maxChars - 2));
}
/**
* Share `available` characters across values by water-filling: values are
* admitted smallest first, each taking the lesser of its full size and an
* equal share of what is left. Requires `available >= Σ floorSize(values)`;
* then every value receives at least its floor.
*/
function allocate(values, available) {
	const sizes = values.map(size);
	const order = sizes.map((_, i) => i).sort((a, b) => sizes[a] - sizes[b]);
	const allocation = new Array(values.length);
	let remaining = available;
	let count = values.length;
	for (const i of order) {
		const alloc = Math.min(sizes[i], Math.floor(remaining / count));
		allocation[i] = alloc;
		remaining -= alloc;
		count--;
	}
	return allocation;
}
function shrinkArray(items, maxChars) {
	const marker = (dropped) => `${TRUNCATION_MARKER} ${__enUS(dropped)} more items`;
	const floors = items.map((item) => floorSize(item, maxChars));
	const skeleton = (count) => 2 + Math.max(0, count - 1);
	let keep = items.length;
	if (floors.reduce((n, f) => n + f, 0) + skeleton(keep) > maxChars) {
		const tail = size(marker(items.length)) + 1;
		let used = skeleton(0) + tail;
		keep = 0;
		while (keep < items.length && used + floors[keep] + 1 <= maxChars) {
			used += floors[keep] + 1;
			keep++;
		}
	}
	const kept = items.slice(0, keep);
	const dropped = items.length - keep;
	const tail = dropped > 0 ? size(marker(dropped)) + (keep > 0 ? 1 : 0) : 0;
	const allocation = allocate(kept, maxChars - skeleton(keep) - tail);
	const out = kept.map((item, i) => shrink(item, allocation[i]));
	if (dropped > 0 && size([...out, marker(dropped)]) <= maxChars) out.push(marker(dropped));
	return out;
}
function shrinkObject(value, maxChars) {
	const entries = Object.entries(value);
	let markerKey = TRUNCATION_MARKER;
	while (markerKey in value) markerKey += " ";
	const keyCost = (key) => size(key) + 1;
	const floors = entries.map(([k, v]) => keyCost(k) + floorSize(v, maxChars));
	const floorOf = (chars) => Math.min(chars, MIN_SLOT, Math.max(2, Math.floor(maxChars / 2)));
	const byValueSize = entries.map((_, i) => i).sort((a, b) => size(entries[b][1]) - size(entries[a][1]));
	const dropped = /* @__PURE__ */ new Set();
	let present = entries.length;
	let floorsSum = floors.reduce((n, f) => n + f, 0);
	let namesLength = 0;
	const noteLength = () => `${__enUS(dropped.size)} keys omitted: `.length + namesLength + 2 * (dropped.size - 1);
	const cost = () => {
		const marker = dropped.size > 0 ? keyCost(markerKey) + floorOf(noteLength() + 2) : 0;
		const count = present + (dropped.size > 0 ? 1 : 0);
		return 2 + Math.max(0, count - 1) + floorsSum + marker;
	};
	for (const i of byValueSize) {
		if (cost() <= maxChars) break;
		dropped.add(i);
		present--;
		floorsSum -= floors[i];
		namesLength += size(entries[i][0]) - 2;
	}
	const kept = entries.filter((_, i) => !dropped.has(i));
	const omitted = [...dropped].sort((a, b) => a - b).map((i) => entries[i][0]);
	const note = `${__enUS(omitted.length)} keys omitted: ${omitted.join(", ")}`;
	if (cost() > maxChars) {
		const room = maxChars - 2 - keyCost(markerKey);
		return room >= 2 ? { [markerKey]: shrink(note, room) } : {};
	}
	const all = [...kept];
	if (omitted.length > 0) all.push([markerKey, note]);
	const fixed = 2 + Math.max(0, all.length - 1) + all.reduce((n, [k]) => n + keyCost(k), 0);
	const allocation = allocate(all.map(([, v]) => v), maxChars - fixed);
	const out = {};
	all.forEach(([k, v], i) => {
		out[k] = shrink(v, allocation[i]);
	});
	return out;
}
//#endregion

const DEFAULT_DESCRIPTION = `Execute code to achieve a goal.

Available:
{{types}}

Write an async arrow function in JavaScript that returns the result.
Do NOT use TypeScript syntax — no type annotations, interfaces, or generics.
Do NOT define named functions then call them — just write the arrow function body directly.

Example: async () => { const r = await codemode.searchWeb({ query: "test" }); return r; }`;

// generateTypes(tools, namespace) of dist/ai.js for tools whose schemas are plain JSON Schema (AI SDK
// jsonSchema() wrappers, which it converts with the same jsonSchemaToTypeString and the same
// description lines): generateTypesFromJsonSchema above with generateTypes' namespace parameter.
function generateTypesForNamespace(tools, namespace = "codemode") {
	let availableTools = "";
	let availableTypes = "";
	for (const [toolName, tool] of Object.entries(tools)) {
		const safeName = sanitizeToolName(toolName);
		const typeName = toPascalCase(safeName);
		try {
			const inputType = jsonSchemaToType(tool.inputSchema, `${typeName}Input`);
			const outputType = tool.outputSchema ? jsonSchemaToType(tool.outputSchema, `${typeName}Output`) : `type ${typeName}Output = unknown`;
			availableTypes += `\n${inputType.trim()}`;
			availableTypes += `\n${outputType.trim()}`;
			const paramLines = (() => {
				try {
					const paramDescs = extractJsonSchemaDescriptions(tool.inputSchema);
					return Object.entries(paramDescs).map(([fieldName, desc]) => `@param input.${fieldName} - ${desc}`);
				} catch {
					return [];
				}
			})();
			const jsdocLines = [];
			if (tool.description?.trim()) jsdocLines.push(escapeJsDoc(tool.description.trim().replace(/\r?\n/g, " ")));
			else jsdocLines.push(escapeJsDoc(toolName));
			for (const pd of paramLines) jsdocLines.push(escapeJsDoc(pd.replace(/\r?\n/g, " ")));
			const jsdocBody = jsdocLines.map((l) => `\t * ${l}`).join("\n");
			availableTools += `\n\t/**\n${jsdocBody}\n\t */`;
			availableTools += `\n\t${safeName}: (input: ${typeName}Input) => Promise<${typeName}Output>;`;
			availableTools += "\n";
		} catch {
			availableTypes += `\ntype ${typeName}Input = unknown`;
			availableTypes += `\ntype ${typeName}Output = unknown`;
			availableTools += `\n\t/**\n\t * ${escapeJsDoc(toolName)}\n\t */`;
			availableTools += `\n\t${safeName}: (input: ${typeName}Input) => Promise<${typeName}Output>;`;
			availableTools += "\n";
		}
	}
	availableTools = `\ndeclare const ${namespace}: {${availableTools}}`;
	return `
${availableTypes}
${availableTools}
  `.trim();
}
