#![cfg(target_os = "macos")]

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use objc2_vision::{
    VNBarcodeObservation, VNDetectBarcodesRequest, VNImageRequestHandler, VNRecognizeTextRequest,
    VNRequest, VNRequestTextRecognitionLevel,
};

pub(crate) fn scan_png(png: &[u8]) -> (Option<String>, Option<String>) {
    let data = NSData::with_bytes(png);
    let options = NSDictionary::new();
    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &data,
        &options,
    );
    let text_req = unsafe { VNRecognizeTextRequest::init(VNRecognizeTextRequest::alloc()) };
    text_req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    text_req.setAutomaticallyDetectsLanguage(true);
    let nl = NSString::from_str("nl-NL");
    let en = NSString::from_str("en-US");
    text_req.setRecognitionLanguages(&NSArray::from_slice(&[&*nl, &*en]));
    let qr_req = unsafe { VNDetectBarcodesRequest::init(VNDetectBarcodesRequest::alloc()) };
    let text_as_req: Retained<VNRequest> =
        Retained::into_super(Retained::into_super(text_req.clone()));
    let qr_as_req: Retained<VNRequest> = Retained::into_super(Retained::into_super(qr_req.clone()));
    if handler
        .performRequests_error(&NSArray::from_slice(&[&*text_as_req, &*qr_as_req]))
        .is_err()
    {
        return (None, None);
    }
    (ocr_text(&text_req), qr_text(&qr_req))
}

fn ocr_text(request: &VNRecognizeTextRequest) -> Option<String> {
    let results = request.results()?;
    let mut lines = Vec::new();
    for observation in results.iter() {
        let Some(candidate) = observation.topCandidates(1).firstObject() else {
            continue;
        };
        let line = candidate.string().to_string();
        if !line.trim().is_empty() {
            lines.push(line);
        }
    }
    let body = lines.join("\n");
    (!body.trim().is_empty()).then_some(body)
}

fn qr_text(request: &VNDetectBarcodesRequest) -> Option<String> {
    let results = unsafe { request.results() }?;
    let mut lines = Vec::new();
    for observation in results.iter() {
        let Some(payload) = (unsafe { observation.payloadStringValue() }) else {
            continue;
        };
        let payload = payload.to_string();
        if payload.trim().is_empty() {
            continue;
        }
        let label = barcode_label(&observation);
        if label == "QR" {
            lines.push(payload);
        } else {
            lines.push(format!("{label}\n{payload}"));
        }
    }
    (!lines.is_empty()).then_some(lines.join("\n\n"))
}

fn barcode_label(observation: &VNBarcodeObservation) -> String {
    let raw = unsafe { observation.symbology() }.to_string();
    raw.strip_prefix("VNBarcodeSymbology")
        .unwrap_or(&raw)
        .to_string()
}
