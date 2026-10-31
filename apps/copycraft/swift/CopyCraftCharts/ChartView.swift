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
    let points: [ChartPoint]
}

/// One row's point: `x` (a number, or days since 1970-01-01 for dates) or `label` (a text x,
/// one bar per row), and `y`.
struct ChartPoint: Decodable, Sendable {
    let label: String?
    let x: Double?
    let y: Double
}

/// One mark, its texts formatted in the app's language. `id` is its place in the points: a
/// bar's identity on the x axis (`key`), so rows with the same text stay separate bars.
struct ChartRow: Identifiable {
    let id: Int
    let key: String
    let label: String
    let x: Double
    let date: Date
    let y: Double
    let xText: String
    let yText: String
}

struct ChartView: View {
    let spec: ChartSpec
    let locale: Locale
    let rows: [ChartRow]

    /// Dates come as whole days (UTC): shown in UTC, so a day never slips to the one before.
    static let utc = TimeZone(identifier: "UTC") ?? TimeZone.current
    /// Bars in view at once; the x axis scrolls sideways to the others (macOS 14 and later; the
    /// app needs macOS 26).
    static let barsInView = 50
    /// A longer bar label is cut, with "…".
    static let labelLength = 24

    init(spec: ChartSpec) {
        self.spec = spec
        let locale = Locale(identifier: spec.locale)
        self.locale = locale
        var rows: [ChartRow] = []
        rows.reserveCapacity(spec.points.count)
        for (index, point) in spec.points.enumerated() {
            let x = point.x ?? 0
            let date = Date(timeIntervalSince1970: x * 86_400)
            let xText: String
            switch spec.kind {
            case "dates": xText = ChartView.day(date, locale: locale)
            case "bars": xText = point.label ?? ""
            default: xText = ChartView.number(x, locale: locale)
            }
            rows.append(ChartRow(
                id: index,
                key: String(index),
                label: point.label ?? "",
                x: x,
                date: date,
                y: point.y,
                xText: xText,
                yText: ChartView.number(point.y, locale: locale)
            ))
        }
        self.rows = rows
    }

    var body: some View {
        content
            .accessibilityLabel(Text(verbatim: spec.series))
            .padding(16)
            .environment(\.locale, locale)
            .environment(\.timeZone, ChartView.utc)
    }

    @ViewBuilder
    private var content: some View {
        switch spec.kind {
        case "dates":
            styled(
                Chart(rows) { row in
                    PointMark(
                        x: .value(spec.xLabel, row.date),
                        y: .value(spec.yLabel, row.y)
                    )
                    .accessibilityLabel(Text(verbatim: row.xText))
                    .accessibilityValue(Text(verbatim: row.yText))
                }
                .chartXAxis { dateAxis }
            )
        case "bars":
            // One bar per row in table order: x is the row's place (never its text, which
            // would stack rows with the same text), labelled with its text.
            styled(
                Chart(rows) { row in
                    BarMark(
                        x: .value(spec.xLabel, row.key),
                        y: .value(spec.yLabel, row.y)
                    )
                    .accessibilityLabel(Text(verbatim: row.xText))
                    .accessibilityValue(Text(verbatim: row.yText))
                }
                .chartXScale(domain: rows.map(\.key))
                .chartXAxis { barAxis }
                .chartScrollableAxes(.horizontal)
                .chartXVisibleDomain(length: min(ChartView.barsInView, max(rows.count, 1)))
            )
        default:
            styled(
                Chart(rows) { row in
                    PointMark(
                        x: .value(spec.xLabel, row.x),
                        y: .value(spec.yLabel, row.y)
                    )
                    .accessibilityLabel(Text(verbatim: "\(spec.xLabel) \(row.xText)"))
                    .accessibilityValue(Text(verbatim: "\(spec.yLabel) \(row.yText)"))
                }
                .chartXAxis { numberAxis }
            )
        }
    }

    /// The axis titles (the column names) and the y axis in the app's language.
    private func styled(_ chart: some View) -> some View {
        chart
            .chartXAxisLabel(spec.xLabel)
            .chartYAxisLabel(spec.yLabel)
            .chartYAxis { numberAxis }
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

    /// A label under every bar: its row's text, upright to fit 50 bars in view.
    private var barAxis: some AxisContent {
        AxisMarks(values: rows.map(\.key)) { value in
            AxisTick()
            AxisValueLabel(collisionResolution: .greedy, orientation: .verticalReversed) {
                if let key = value.as(String.self), let place = Int(key),
                   rows.indices.contains(place)
                {
                    Text(verbatim: ChartView.short(rows[place].label))
                        .font(.caption2)
                        .lineLimit(1)
                }
            }
        }
    }

    /// `text` cut at `labelLength` characters, with "…".
    static func short(_ text: String) -> String {
        text.count > labelLength ? String(text.prefix(labelLength - 1)) + "…" : text
    }

    /// Grid lines, ticks and days formatted in the app's language.
    private var dateAxis: some AxisContent {
        AxisMarks { value in
            AxisGridLine()
            AxisTick()
            AxisValueLabel {
                if let date = value.as(Date.self) {
                    Text(verbatim: ChartView.day(date, locale: locale))
                }
            }
        }
    }

    /// A number in the app's language ("1.234,5" in Dutch, "1,234.5" in English).
    static func number(_ value: Double, locale: Locale) -> String {
        value.formatted(.number.locale(locale))
    }

    /// A day in the app's language ("5 mrt 2026" / "Mar 5, 2026"), read in UTC.
    static func day(_ date: Date, locale: Locale) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = utc
        let style = Date.FormatStyle(
            date: .abbreviated,
            time: .omitted,
            locale: locale,
            calendar: calendar,
            timeZone: utc
        )
        return date.formatted(style)
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
