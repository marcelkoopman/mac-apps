// The table window's chart (src/macos_chart.rs): a SwiftUI view with Swift Charts in an
// NSHostingView, drawn from the flat JSON of src/chart_data.rs. Every text comes translated from
// Rust; this file has none of its own. Nothing is logged, printed or stored here, and nothing
// goes over the network: the JSON comes in, the view draws it, and it is gone when the view is.
// Called on the main thread only (AppKit).

import AppKit
import Charts
import Foundation
import SwiftUI

/// The JSON of `ChartData::to_json` (keys in snake case).
struct ChartSpec: Decodable, Sendable {
    let kind: String
    let locale: String
    let xLabel: String
    let yLabel: String
    let series: String
    let unit: String?
    let skipped: Int
    let points: [ChartPoint]
}

/// A bar or line point (`label`, `value`, `date`, `other`) or a scatter point (`x`, `y`).
struct ChartPoint: Decodable, Sendable {
    let label: String?
    let value: Double?
    let date: String?
    let other: Bool?
    let x: Double?
    let y: Double?
}

/// One mark, its texts formatted in the app's language.
struct ChartRow: Identifiable {
    let id: Int
    let label: String
    let value: Double
    let valueText: String
    let x: Double
    let y: Double
    let xText: String
}

struct ChartView: View {
    let spec: ChartSpec
    let locale: Locale
    let rows: [ChartRow]

    init(spec: ChartSpec) {
        self.spec = spec
        let locale = Locale(identifier: spec.locale)
        self.locale = locale
        var rows: [ChartRow] = []
        rows.reserveCapacity(spec.points.count)
        for (index, point) in spec.points.enumerated() {
            let value = point.value ?? 0
            let x = point.x ?? 0
            let y = point.y ?? 0
            let isScatter = spec.kind == "scatter"
            rows.append(ChartRow(
                id: index,
                label: ChartView.label(point, unit: spec.unit, locale: locale),
                value: value,
                valueText: ChartView.number(isScatter ? y : value, locale: locale),
                x: x,
                y: y,
                xText: ChartView.number(x, locale: locale)
            ))
        }
        self.rows = rows
    }

    var body: some View {
        content
            .accessibilityLabel(Text(verbatim: spec.series))
            .padding(16)
            .environment(\.locale, locale)
    }

    @ViewBuilder
    private var content: some View {
        switch spec.kind {
        case "scatter":
            styled(
                Chart(rows) { row in
                    PointMark(
                        x: .value(spec.xLabel, row.x),
                        y: .value(spec.yLabel, row.y)
                    )
                    .foregroundStyle(by: .value(spec.series, spec.series))
                    .accessibilityLabel(Text(verbatim: "\(spec.xLabel) \(row.xText)"))
                    .accessibilityValue(Text(verbatim: "\(spec.yLabel) \(row.valueText)"))
                }
                .chartXAxis { numberAxis }
            )
        case "line":
            styled(
                Chart(rows) { row in
                    LineMark(
                        x: .value(spec.xLabel, row.label),
                        y: .value(spec.yLabel, row.value)
                    )
                    .symbol(.circle)
                    .foregroundStyle(by: .value(spec.series, spec.series))
                    .accessibilityLabel(Text(verbatim: row.label))
                    .accessibilityValue(Text(verbatim: row.valueText))
                }
            )
        default:
            styled(
                Chart(rows) { row in
                    BarMark(
                        x: .value(spec.xLabel, row.label),
                        y: .value(spec.yLabel, row.value)
                    )
                    .foregroundStyle(by: .value(spec.series, spec.series))
                    .accessibilityLabel(Text(verbatim: row.label))
                    .accessibilityValue(Text(verbatim: row.valueText))
                }
            )
        }
    }

    /// The axis titles, the value axis in the app's language and the legend, on each chart.
    private func styled(_ chart: some View) -> some View {
        chart
            .chartXAxisLabel(spec.xLabel)
            .chartYAxisLabel(spec.yLabel)
            .chartYAxis { numberAxis }
            .chartLegend(position: .bottom, alignment: .leading)
    }

    /// Grid lines, ticks and numbers formatted in the app's language.
    private var numberAxis: some AxisContent {
        AxisMarks { value in
            AxisGridLine()
            AxisTick()
            AxisValueLabel {
                if let number = value.as(Double.self) {
                    Text(verbatim: ChartView.number(number, locale: locale))
                }
            }
        }
    }

    /// A number in the app's language ("1.234,5" in Dutch, "1,234.5" in English).
    static func number(_ value: Double, locale: Locale) -> String {
        value.formatted(.number.locale(locale))
    }

    /// The point's label: a date group's first day formatted per its unit in the app's language,
    /// else the label as Rust gave it (a group's text, or "Other" already translated).
    static func label(_ point: ChartPoint, unit: String?, locale: Locale) -> String {
        let fallback = point.label ?? ""
        guard let text = point.date, let date = day(text) else { return fallback }
        switch unit {
        case "day":
            return date.formatted(.dateTime.day().month(.abbreviated).year().locale(locale))
        case "month":
            return date.formatted(.dateTime.month(.abbreviated).year().locale(locale))
        case "year":
            return date.formatted(.dateTime.year().locale(locale))
        default:
            return fallback
        }
    }

    /// Midnight (here) of `yyyy-mm-dd`.
    static func day(_ text: String) -> Date? {
        let parts = text.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        let components = DateComponents(year: parts[0], month: parts[1], day: parts[2])
        return Calendar(identifier: .gregorian).date(from: components)
    }
}

/// The spec in `bytes` (UTF-8 JSON, `len` bytes, read in place), or nil when it does not decode.
private func decodeSpec(_ bytes: UnsafePointer<UInt8>?, _ len: Int) -> ChartSpec? {
    guard let bytes, len > 0 else { return nil }
    let data = Data(
        bytesNoCopy: UnsafeMutableRawPointer(mutating: bytes),
        count: len,
        deallocator: .none
    )
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try? decoder.decode(ChartSpec.self, from: data)
}

/// A retained NSHostingView drawing the chart in `bytes`, or nil when the JSON does not decode.
/// The caller releases it with `cc_chart_release`. Main thread only.
@_cdecl("cc_chart_create")
public func ccChartCreate(_ bytes: UnsafePointer<UInt8>?, _ len: Int) -> UnsafeMutableRawPointer? {
    guard let spec = decodeSpec(bytes, len) else { return nil }
    let address = MainActor.assumeIsolated { () -> Int in
        let host = NSHostingView(rootView: ChartView(spec: spec))
        // Rust sizes the view (autoresizing): the hosting view adds no size constraints.
        host.sizingOptions = []
        return Int(bitPattern: Unmanaged.passRetained(host).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Draw the chart in `bytes` in the view from `cc_chart_create`. False when the JSON does not
/// decode (the view keeps the chart it had). Main thread only.
@_cdecl("cc_chart_update")
public func ccChartUpdate(
    _ view: UnsafeMutableRawPointer?,
    _ bytes: UnsafePointer<UInt8>?,
    _ len: Int
) -> Bool {
    guard let view, let spec = decodeSpec(bytes, len) else { return false }
    let address = Int(bitPattern: view)
    return MainActor.assumeIsolated { () -> Bool in
        guard let pointer = UnsafeMutableRawPointer(bitPattern: address) else { return false }
        let host = Unmanaged<NSHostingView<ChartView>>.fromOpaque(pointer).takeUnretainedValue()
        host.rootView = ChartView(spec: spec)
        return true
    }
}

/// Release the view from `cc_chart_create` (the reference it was returned with). Main thread only.
@_cdecl("cc_chart_release")
public func ccChartRelease(_ view: UnsafeMutableRawPointer?) {
    guard let view else { return }
    Unmanaged<NSHostingView<ChartView>>.fromOpaque(view).release()
}
