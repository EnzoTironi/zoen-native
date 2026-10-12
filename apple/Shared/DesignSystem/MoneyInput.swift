import Foundation

enum MoneyInput {
    static func text(for cents: Int64, locale: Locale) -> String {
        let magnitude = cents.magnitude
        let fraction = magnitude % 100
        let separator = locale.decimalSeparator ?? "."
        return "\(cents < 0 ? "-" : "")\(magnitude / 100)\(separator)\(fraction < 10 ? "0" : "")\(fraction)"
    }

    static func cents(from text: String, locale: Locale) -> Int64? {
        var input = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard input.utf8.count <= 64 else { return nil }
        let negative = input.first == "-"
        if negative || input.first == "+" { input.removeFirst() }
        let parts = input.components(separatedBy: locale.decimalSeparator ?? ".")
        guard (1...2).contains(parts.count) else { return nil }
        let fraction = parts.count == 2 ? parts[1] : ""
        guard fraction.count <= 2, !parts[0].isEmpty || !fraction.isEmpty else { return nil }
        let digits = parts[0] + fraction + String(repeating: "0", count: 2 - fraction.count)
        let limit = UInt64(Int64.max) + (negative ? 1 : 0)
        var magnitude: UInt64 = 0
        for digit in digits.utf8 {
            guard (48...57).contains(digit) else { return nil }
            let (shifted, shiftOverflow) = magnitude.multipliedReportingOverflow(by: 10)
            let (next, addOverflow) = shifted.addingReportingOverflow(UInt64(digit - 48))
            guard !shiftOverflow, !addOverflow, next <= limit else { return nil }
            magnitude = next
        }
        if negative && magnitude == UInt64(Int64.max) + 1 { return Int64.min }
        let cents = Int64(magnitude)
        return negative ? -cents : cents
    }

    static func sumCents<S: Sequence>(_ values: S) -> Int64? where S.Element == Int64 {
        var sum = Decimal.zero
        for value in values {
            var term = NSDecimalNumber(value: value).decimalValue
            var next = Decimal.zero
            guard NSDecimalAdd(&next, &sum, &term, .plain) == .noError else { return nil }
            sum = next
        }
        var lower = NSDecimalNumber(value: Int64.min).decimalValue
        var upper = NSDecimalNumber(value: Int64.max).decimalValue
        guard NSDecimalCompare(&sum, &lower) != .orderedAscending,
              NSDecimalCompare(&sum, &upper) != .orderedDescending else { return nil }
        return NSDecimalNumber(decimal: sum).int64Value
    }

    static func differenceCents(_ amount: Int64, _ other: Int64) -> Int64? {
        let (difference, overflow) = amount.subtractingReportingOverflow(other)
        return overflow ? nil : difference
    }
}
