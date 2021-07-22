#version 450

layout(input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput diffuse; // ignored
layout(input_attachment_index = 0, set = 0, binding = 1) uniform subpassInput normal; // ignored
layout(input_attachment_index = 0, set = 0, binding = 2) uniform subpassInput depth; // ignored
layout(set = 0, binding = 3) uniform sampler2D shadow;

layout(push_constant) uniform LightBuffer {
    mat4 light_to_screen;
    vec4 sunlight_direction;
} light_buffer;

layout(location = 0) out vec3 fragColor;

void main() {

    if (gl_FrontFacing) {
        fragColor = 0.1 * vec3(0.1, 0.2, 0.4);
    } else {
        fragColor = 0.1 * vec3(0.4, 0.2, 0.1);
    }
}
