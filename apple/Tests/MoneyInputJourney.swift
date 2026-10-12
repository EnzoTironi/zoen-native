import Foundation

private struct JourneyFailure: Error, CustomStringConvertible {
    let description: String
}

@main
struct MoneyInputJourney {
    static func main() throws {
        var passed = 0
        func expect(_ condition: Bool, _ name: String) throws {
            guard condition else { throw JourneyFailure(description: name) }
            passed += 1
            print("PASS \(name)")
        }
        let english = Locale(identifier: "en_US")
        let portuguese = Locale(identifier: "pt_BR")
        for locale in [english, portuguese] {
            for cents: Int64 in [0, 1, -1, 99, -99, 100, -100, 123456, -123456, 9007199254740993, Int64.min, Int64.max] {
                let shown = MoneyInput.text(for: cents, locale: locale)
                try expect(MoneyInput.cents(from: shown, locale: locale) == cents,
                           "unchanged price keeps exact cents: \(locale.identifier) / \(cents)")
            }
        }
        try expect(MoneyInput.cents(from: "  +0012.3 \n", locale: english) == 1230, "spaces, sign and one fractional digit")
        try expect(MoneyInput.cents(from: "-12,34", locale: portuguese) == -1234, "localized negative credit")
        try expect(MoneyInput.cents(from: "12", locale: english) == 1200, "whole-unit input")
        try expect(MoneyInput.cents(from: ".50", locale: english) == 50, "fractional-only amount")
        try expect(MoneyInput.cents(from: "-,50", locale: portuguese) == -50, "localized fractional-only credit")
        for input in ["", "-", ".", "1.234", "1.2.3", "NaN", "Infinity", "1e3", "1abc", "1,000.00", "1 000.00", "92233720368547758.08", "-92233720368547758.09", "184467440737095516160.00"] {
            try expect(MoneyInput.cents(from: input, locale: english) == nil, "invalid or out-of-range input refuses: \(input)")
        }
        try expect(MoneyInput.cents(from: "12.34", locale: portuguese) == nil, "other locale separator is not reinterpreted")
        try expect(MoneyInput.cents(from: String(repeating: "0", count: 65), locale: english) == nil, "oversized input refuses before accumulation")
        let sums: [([Int64], Int64?)] = [
            ([], 0), ([100, -25], 75),
            ([Int64.max, 1, -1], Int64.max), ([-1, 1, Int64.max], Int64.max),
            ([Int64.min, -1, 1], Int64.min), ([1, -1, Int64.min], Int64.min),
            ([Int64.max, Int64.max, Int64.min], Int64.max - 1),
            ([Int64.min, Int64.min, Int64.max, Int64.max], -2),
            ([Int64.max, 1], nil), ([Int64.min, -1], nil)
        ]
        for (values, total) in sums {
            try expect(MoneyInput.sumCents(values) == total, "signed subtotal: \(values)")
        }
        try expect(MoneyInput.differenceCents(1500, 1388) == 112, "ordinary budget remainder")
        try expect(MoneyInput.differenceCents(Int64.max, Int64.min) == nil, "unrepresentable budget difference refuses")
        try expect(MoneyInput.differenceCents(Int64.min, Int64.max) == nil, "unrepresentable negative difference refuses")
        try expect(MoneyInput.differenceCents(Int64.min, Int64.min) == 0, "equal minimum values have zero difference")
        print("RESULT \(passed) passed; 0 failed")
    }
}
