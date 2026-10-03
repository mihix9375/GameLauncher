// Numberへの変換を避け、大きな整数の下位桁を失わない表示用整形。
export function formatScore(value) {
	const text = String(value ?? "").trim();
	const match = /^([+-]?)(\d+)(?:\.(\d*))?(?:e([+-]?\d+))?$/i.exec(text);
	if (!match) return text || "—";
	let digits = (match[2] + (match[3] || "")).replace(/^0+/, "");
	if (!digits) return "0";
	const exponent = Number(match[4] || 0) - (match[3]?.length || 0) + digits.length - 1;
	const sign = match[1] === "-" ? "-" : "";
	if (exponent >= 9 || exponent <= -5) {
		// 4有効桁に四捨五入するのは表示だけ。保存値・順位比較には使わない。
		let head = digits.slice(0, 4).padEnd(4, "0");
		if (digits.length > 4 && digits[4] >= "5") head = String(Number(head) + 1);
		const overflow = head.length > 4;
		if (overflow) head = "1000";
		const fraction = head.slice(1).replace(/0+$/, "");
		return `${sign}${head[0]}${fraction ? `.${fraction}` : ""}e${exponent + Number(overflow)}`;
	}
	const point = exponent + 1;
	let plain = point <= 0 ? `0.${"0".repeat(-point)}${digits}`
		: point >= digits.length ? digits + "0".repeat(point - digits.length)
		: `${digits.slice(0, point)}.${digits.slice(point)}`;
	if (plain.includes(".")) plain = plain.replace(/0+$/, "").replace(/\.$/, "");
	const [integer, fraction] = plain.split(".");
	return sign + integer.replace(/\B(?=(\d{3})+(?!\d))/g, ",") + (fraction ? `.${fraction}` : "");
}
