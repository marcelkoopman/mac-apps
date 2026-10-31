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

/// What the chart shows: when it changes (other columns, the window before or after), the
/// marks move to their new places.
struct ChartIdentity: Equatable {
    let kind: String
    let xLabel: String
    let yLabel: String
    let xs: [Double]
    let ys: [Double]
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
    let identity: ChartIdentity
    /// System Settings › Accessibility › Display › Reduce motion: then nothing moves.
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Dates come as whole days (UTC): shown in UTC, so a day never slips to the one before.
    static let utc = TimeZone(identifier: "UTC") ?? TimeZone.current
    /// The most points drawn: Rust sends a window of 50 rows; more are never drawn.
    static let maxPoints = 50
    /// A longer bar label is cut, with "…".
    static let labelLength = 24
    /// The marks moving to a new chart.
    static let change = Animation.easeInOut(duration: 0.35)
    /// Bars and the area under a line: the accent colour, fading downwards (system colours, so
    /// light and dark mode both work).
    static let fill = LinearGradient(
        colors: [Color.accentColor, Color.accentColor.opacity(0.35)],
        startPoint: .top,
        endPoint: .bottom
    )
    static let area = LinearGradient(
        colors: [Color.accentColor.opacity(0.3), Color.accentColor.opacity(0)],
        startPoint: .top,
        endPoint: .bottom
    )

    init(spec: ChartSpec) {
        self.spec = spec
        let locale = Locale(identifier: spec.locale)
        self.locale = locale
        var rows: [ChartRow] = []
        let points = spec.points.prefix(ChartView.maxPoints)
        rows.reserveCapacity(points.count)
        for (index, point) in points.enumerated() {
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
        self.identity = ChartIdentity(
            kind: spec.kind,
            xLabel: spec.xLabel,
            yLabel: spec.yLabel,
            xs: rows.map(\.x),
            ys: rows.map(\.y)
        )
    }

    var body: some View {
        content
            .animation(reduceMotion ? nil : ChartView.change, value: identity)
            .accessibilityLabel(Text(verbatim: spec.series))
            .padding(16)
            .environment(\.locale, locale)
            .environment(\.timeZone, ChartView.utc)
    }

    @ViewBuilder
    private var content: some View {
        switch spec.kind {
        case "dates":
            // A smooth line through the points, over an area fading downwards. Only the
            // points are read by VoiceOver.
            styled(
                Chart(rows) { row in
                    AreaMark(
                        x: .value(spec.xLabel, row.date),
                        y: .value(spec.yLabel, row.y)
                    )
                    .interpolationMethod(.catmullRom)
                    .foregroundStyle(ChartView.area)
                    .accessibilityHidden(true)
                    LineMark(
                        x: .value(spec.xLabel, row.date),
                        y: .value(spec.yLabel, row.y)
                    )
                    .interpolationMethod(.catmullRom)
                    .lineStyle(StrokeStyle(lineWidth: 2))
                    .foregroundStyle(Color.accentColor)
                    .accessibilityHidden(true)
                    PointMark(
                        x: .value(spec.xLabel, row.date),
                        y: .value(spec.yLabel, row.y)
                    )
                    .symbolSize(24)
                    .foregroundStyle(Color.accentColor)
                    .accessibilityLabel(Text(verbatim: row.xText))
                    .accessibilityValue(Text(verbatim: row.yText))
                }
                .chartXAxis { dateAxis }
            )
        case "bars":
            // One bar per row of the window in table order: x is the row's place (never its
            // text, which would stack rows with the same text), labelled with its text.
            styled(
                Chart(rows) { row in
                    BarMark(
                        x: .value(spec.xLabel, row.key),
                        y: .value(spec.yLabel, row.y)
                    )
                    .foregroundStyle(ChartView.fill)
                    .cornerRadius(3)
                    .accessibilityLabel(Text(verbatim: row.xText))
                    .accessibilityValue(Text(verbatim: row.yText))
                }
                .chartXScale(domain: rows.map(\.key))
                .chartXAxis { barAxis }
            )
        default:
            styled(
                Chart(rows) { row in
                    PointMark(
                        x: .value(spec.xLabel, row.x),
                        y: .value(spec.yLabel, row.y)
                    )
                    .foregroundStyle(Color.accentColor.opacity(0.8))
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

    /// A label under a bar: its row's text, cut short; labels that would overlap are left out.
    private var barAxis: some AxisContent {
        AxisMarks(values: rows.map(\.key)) { value in
            AxisTick()
            AxisValueLabel(collisionResolution: .greedy) {
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

/// The most JSON bytes read: 50 points with cut labels stay far below this.
private let maxSpecBytes = 1 << 20

/// The spec in `bytes` (UTF-8 JSON, `len` bytes, read in place), or nil when it does not decode.
private func decodeSpec(_ bytes: UnsafePointer<UInt8>?, _ len: Int) -> ChartSpec? {
    guard let bytes, len > 0, len <= maxSpecBytes else { return nil }
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
