// Argent 0.29 describe formatter contract. No device or provider calls here.
export function argentBody(result) {
  if (!result || result.isError === true) throw new Error("argent_tool_error");
  if (result.structuredContent) {
    if (result.structuredContent.isError === true) throw new Error("argent_tool_error");
    return result.structuredContent;
  }
  if (!result.content) return result;
  const texts = result.content.filter((part) => part.type === "text");
  if (texts.length !== 1) throw new Error("ambiguous_argent_result");
  try {
    return JSON.parse(texts[0].text);
  } catch {
    throw new Error("invalid_argent_result");
  }
}

function quoted(input) {
  if (!input.startsWith('"')) throw new Error("invalid_describe_text");
  let value = "";
  for (let index = 1; index < input.length; index++) {
    const char = input[index];
    if (char === '"') return [value, input.slice(index + 1).trimStart()];
    if (char !== "\\") value += char;
    else {
      const escaped = input[++index];
      const known = { "\\": "\\", n: "\n", r: "\r", t: "\t" };
      if (!Object.hasOwn(known, escaped)) throw new Error("invalid_describe_text");
      value += known[escaped];
    }
  }
  throw new Error("invalid_describe_text");
}

/** Only the current XCUITest projection guarantees its disabled flag from isEnabled.
 * AX/native simulator adapters currently omit it. Absence there is not enabled=true.
 */
export function argentControls(result) {
  const body = argentBody(result);
  if (body.source !== "xcuitest-runner") throw new Error("unsupported_actionability_source");
  if (body.should_restart || body.hint) throw new Error("degraded_argent_observation");
  if (typeof body.description !== "string" || body.description.length > 128 * 1024)
    throw new Error("invalid_describe_text");
  const lines = body.description.split("\n");
  if (lines[0] !== "Source: xcuitest-runner" || !lines.includes("Mode: nested"))
    throw new Error("invalid_describe_text");
  const root = lines.findIndex((line) => line.startsWith("ROOT  "));
  if (root < 0) throw new Error("invalid_describe_text");
  return lines
    .slice(root + 1)
    .filter((line) => line.trim())
    .map((line) => {
      const match = /^ +([\w-]+)(.*?)  \(([-\d.]+), ([-\d.]+), ([-\d.]+), ([-\d.]+)\)$/.exec(line);
      if (!match) throw new Error("invalid_describe_text");
      const frame = match.slice(3).map(Number);
      if (!frame.every((value) => Number.isFinite(value) && value >= 0 && value <= 1))
        throw new Error("invalid_describe_frame");
      if (frame[0] + frame[2] / 2 > 1 || frame[1] + frame[3] / 2 > 1)
        throw new Error("invalid_describe_frame");
      let rest = match[2].trim();
      let label = "";
      if (rest.startsWith('"')) [label, rest] = quoted(rest);
      const attributes = {};
      while (rest.startsWith("value=") || rest.startsWith("id=")) {
        const name = rest.startsWith("id=") ? "id" : "value";
        if (Object.hasOwn(attributes, name)) throw new Error("invalid_describe_text");
        [attributes[name], rest] = quoted(rest.slice(name.length + 1));
      }
      if (rest && !/^\[(?:[\w=-]+)(?:,[\w=-]+)*\]$/.test(rest))
        throw new Error("invalid_describe_text");
      const flags = new Set(rest ? rest.slice(1, -1).split(",") : []);
      return {
        role: match[1],
        label,
        identifier: attributes.id,
        frame: { x: frame[0], y: frame[1], width: frame[2], height: frame[3] },
        enabled: !flags.has("disabled"),
        visible: frame[2] > 0 && frame[3] > 0,
        sensitive: flags.has("password"),
      };
    });
}
