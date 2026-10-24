//! Image ▾ steps and files, and the resize prompt. Part of the catalog in `strings.rs`, whose glossary holds here too.

pub(super) const STRINGS: &[(&str, &str, &str)] = &[
    // Image steps and files
    (
        "image_longest_side",
        "Langste zijde {0}",
        "Longest side {0}",
    ),
    ("image_width", "Breedte {0}", "Width {0}"),
    ("image_height", "Hoogte {0}", "Height {0}"),
    (
        "image_rotate_left",
        "Draai 90° naar links",
        "Rotate 90° Left",
    ),
    (
        "image_rotate_right",
        "Draai 90° naar rechts",
        "Rotate 90° Right",
    ),
    (
        "image_flip_horizontal",
        "Spiegel horizontaal",
        "Flip Horizontal",
    ),
    ("image_flip_vertical", "Spiegel verticaal", "Flip Vertical"),
    (
        "image_remove_metadata",
        "Verwijder metadata",
        "Remove Metadata",
    ),
    ("image_grayscale", "Grijswaarden", "Grayscale"),
    (
        "image_resized_to",
        "Formaat gewijzigd naar {0}×{1}",
        "Resized to {0}×{1}",
    ),
    (
        "image_rotated_left",
        "90° naar links gedraaid",
        "Rotated 90° left",
    ),
    (
        "image_rotated_right",
        "90° naar rechts gedraaid",
        "Rotated 90° right",
    ),
    (
        "image_flipped_horizontally",
        "Horizontaal gespiegeld",
        "Flipped horizontally",
    ),
    (
        "image_flipped_vertically",
        "Verticaal gespiegeld",
        "Flipped vertically",
    ),
    (
        "image_metadata_removed",
        "Metadata verwijderd",
        "Metadata removed",
    ),
    (
        "image_already_that_size",
        "Al dat formaat of kleiner",
        "Already that size or smaller",
    ),
    ("image_resize", "Wijzig formaat", "Resize"),
    ("image_file_as_is", "{0} (ongewijzigd)", "{0} (as it is)"),
    ("image_jpeg_high", "JPEG — hoog (0.9)", "JPEG — high (0.9)"),
    (
        "image_jpeg_medium",
        "JPEG — gemiddeld (0.7)",
        "JPEG — medium (0.7)",
    ),
    (
        "image_cannot_read",
        "Kan deze afbeelding niet lezen",
        "Can't read this image",
    ),
    (
        "image_cannot_write",
        "Kan deze afbeelding niet schrijven",
        "Can't write this image",
    ),
    ("image_aspect_line", "Beeldverhouding: {0}", "Aspect: {0}"),
    ("image_pixels_line", "Pixels: {0}", "Pixels: {0}"),
    // Resize prompt
    ("resize_to", "Formaat wijzigen naar", "Resize to"),
    (
        "resize_prompt",
        "Een breedte (1200 of w 1200) of een hoogte (h 800), in pixels. De afbeelding behoudt haar verhoudingen en wordt nooit groter.",
        "A width (1200 or w 1200) or a height (h 800), in pixels. The image keeps its proportions and never gets larger.",
    ),
    (
        "resize_invalid",
        "Dat is geen formaat. Typ een breedte zoals 1200 of w 1200, of een hoogte zoals h 800.",
        "That is not a size. Type a width like 1200 or w 1200, or a height like h 800.",
    ),
];
