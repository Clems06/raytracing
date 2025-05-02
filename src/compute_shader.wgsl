fn init_rand(invocation_id : u32, seed : vec4<f32>) -> vec2f {
  var rand_seed = seed.xz;
  rand_seed = fract(rand_seed * cos(35.456+f32(invocation_id) * seed.yw));
  rand_seed = fract(rand_seed * cos(41.235+f32(invocation_id) * seed.xw));
  return rand_seed;
}

fn rand(rand_seed: ptr<function,vec2f>) -> f32 {
  (*rand_seed).x = fract(cos(dot(*rand_seed, vec2<f32>(23.14077926, 232.61690225))) * 136.8168);
  (*rand_seed).y = fract(cos(dot(*rand_seed, vec2<f32>(54.47856553, 345.84153136))) * 534.7645);
  return (*rand_seed).y;
}

struct Sphere {
    pos: vec3f,
    radius_squared: f32,
    color: vec4f,
    mirror: f32
    //brightness: f32,
}


struct Triangle {
    pos_a: vec3f,
    mirror: f32,
    pos_b: vec3f,
    pad: f32,
    pos_c: vec3f,
    pad2: f32,
    color: vec4f,

}


struct Ray {
    origin: vec3f,
    direction: vec3f
}

struct Cast_Result {
    color: vec4f,
    bounce: bool,
    bounce_ray: Ray,
}

struct Uniforms {
    screen_width: f32,
    screen_height: f32,
    do_merging: f32,
    time: f32,
    camera_position: vec4<f32>,
    camera_direction: vec4<f32>,
    padded_bytes_per_row: u32,
    bounce_num: u32,
    pad: vec2<f32>

};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> spheres: array<Sphere>;
@group(0) @binding(3) var<storage, read> triangles: array<Triangle>;


const rays_per_pixel: u32 = 5;

fn ray_hits_sphere(sphere: Sphere, ray: Ray, a: f32) -> f32 {
    //Verifie x^2 + y^2 + z^2 = R^2 et (x0, y0, z0) + (dx, dy, dz) * d = (x, y ,z)
    let oc = ray.origin - sphere.pos;

    //let a = dot(ray.direction, ray.direction);
    let b = dot(oc, ray.direction);
    let c = dot(oc, oc) - sphere.radius_squared;
    let delta = b * b - a * c;
    var out: f32;
    if delta < 0. {
        out = -1.;
    } else {
        let d1 = (-b - sqrt(delta)) / (a);
        if d1 > 0. {
            out = d1;
            //Some((x0 + d1 * dx, y0 + d1 * dy, z0 + d1 * dz))
        } else {
            let d2 = (-b + sqrt(delta)) / (a);
            out = select(-1.0, d2, d2>0.0);
        }
    }
    return out;

}

fn bounced_ray_sphere(ray: Ray, dist: f32, sphere: Sphere, rand_seed: ptr<function,vec2f>) -> Ray {
    let point_hit = dist*ray.direction+ray.origin;
    let normal_vector = normalize(point_hit - sphere.pos);
    var new_dir: vec3f;
    /*if sphere.mirror < rand(rand_seed) {
        new_dir = vec3f(rand(rand_seed), rand(rand_seed), rand(rand_seed));
        //new_dir = normalize(normal_vector + vec3f(rand(rand_seed), rand(rand_seed), rand(rand_seed))*2);
        if dot(new_dir, normal_vector) < 0 {new_dir = - new_dir;}
    } else {
        new_dir = ray.direction - 2 * dot(ray.direction, normal_vector) * normal_vector;

    }*/

    let perfect_reflection = ray.direction - 2 * dot(ray.direction, normal_vector) * normal_vector;
    let diffuse_reflection = vec3f(rand(rand_seed), rand(rand_seed), rand(rand_seed));
    if dot(new_dir, normal_vector) < 0 {new_dir = - new_dir;}

    new_dir = perfect_reflection * sphere.mirror + diffuse_reflection * (1 - sphere.mirror);

    return Ray(point_hit + 0.0001 * normal_vector, new_dir);
}

//Premier est la distance, 3 autres vecteur normal
fn ray_hits_triangle(triangled: Triangle, ray: Ray) -> vec4f{
    let edgeAB = triangled.pos_b - triangled.pos_a;
    let edgeAC = triangled.pos_c - triangled.pos_a;
    let normal = cross(edgeAB, edgeAC);

    let ao = ray.origin - triangled.pos_a;
    let dao = cross(ao, ray.direction);


    let face_normal = select(normal, -normal, dot(ray.direction, normal) > 0.0);
    //let determinant = -dot(ray.direction, face_normal);
    let determinant = -dot(ray.direction, normal);


    if abs(determinant) <= 0.0001 {return vec4f(-1.);}

    //let dst = dot(ao, face_normal)/determinant;
    let dst = dot(ao, normal)/determinant;

    if dst <= 0.0 {
            return vec4f(-1.);
        }

    let u = dot(edgeAC, dao)/determinant;
    let v = -dot(edgeAB, dao)/determinant;
    let w = 1 - u - v;

    if u < 0.0 || v < 0.0 || w < 0. {
            return vec4f(-1.);
    }

    return vec4f(dst, face_normal);
    //return vec4f(dst, normal);
}


fn background(ray: Ray) -> vec4f {
        let sun_dir = normalize(vec3<f32>(1.0, 1.0, 0.0));
        if dot(sun_dir, ray.direction) > 0.999 {
            //Sun
            return vec4<f32>(2.0, 2.0, 2.0, 1.0);
            //return vec4<f32>(0.95*5, .78*5, .24*5, 1.0);
        }

        let a = dot(vec3<f32>(0.0, 0.0, 1.0), ray.direction);
        if a > -0.1 {
            //Sky
            let f = exp(-100*(a+0.1));
            return (vec4<f32>(1., 1., 1.0, 1.0) * f + vec4<f32>(.55, .79, 1.0, 0.1) * (1 - f))*0.8;
            //return vec4<f32>(0., 0., 0.0, 1.0);
        }
        //Ground
        return vec4<f32>(0.5, 0.5, 0.5, 1.0)*0.8;
        //return vec4<f32>(0., 0., 0.0, 1.0);
    }

fn trace_ray(ray: Ray, rand_seed: ptr<function,vec2f>) -> Cast_Result {
    var min_distance = -1.;
    var sphere_bounced: u32 = 0;
    var result = Cast_Result(background(ray), false, Ray(vec3f(0.0), vec3f(0.0)));
    let a = dot(ray.direction, ray.direction);
    for (var i: u32 = 0; i < arrayLength(&spheres); i++) {
        let hit = ray_hits_sphere(spheres[i], ray, a);
        if (hit>0 && (min_distance<0 || hit < min_distance)) {
            result.bounce = true;
            sphere_bounced = i;
            min_distance = hit;
        }
    }
    if result.bounce {
        result.color = spheres[sphere_bounced].color;
        result.bounce_ray = bounced_ray_sphere(ray, min_distance, spheres[sphere_bounced], rand_seed);
    }
    for (var i: u32 = 0; i < arrayLength(&triangles); i++) {
        let a = ray_hits_triangle(triangles[i], ray);
        if (a.x>0 && (min_distance<0 || a.x < min_distance)) {
            result.color = triangles[i].color;
            result.bounce = true;
            min_distance = a.x;


            let point_hit = a.x*ray.direction+ray.origin;
            let normal_vector = a.yzw;
            var new_dir: vec3f;
            if triangles[i].mirror < rand(rand_seed) {
                new_dir = vec3f(rand(rand_seed), rand(rand_seed), rand(rand_seed));
                new_dir = normalize(new_dir);
                if dot(new_dir, normal_vector) < 0.0 {
                    new_dir = -new_dir;
                }
            } else {
                // Perfect mirror reflection
                new_dir = normalize(ray.direction - 2.0 * dot(ray.direction, normal_vector) * normal_vector);
            }

            // Slightly offset the ray origin to prevent self-intersection
            result.bounce_ray = Ray(point_hit + 0.001 * normal_vector, normalize(new_dir));

        }
       }
    return result;
}

fn trace_ray_bounces(ray: Ray, rand_seed: ptr<function,vec2f>) -> vec4f {
    var ray_cast = ray;
    var result = trace_ray(ray_cast, rand_seed);
    var out = vec4f(1.0, 1.0, 1.0, 1.0);
    for (var bounces: u32 = 0; bounces < uniforms.bounce_num; bounces++) {
        out *= result.color;
        if (!result.bounce){return out;}
        ray_cast = result.bounce_ray;
        result = trace_ray(ray_cast, rand_seed);
    }
    return out;


}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {


    /*let index = id.y * u32(uniforms.screen_width) + id.x;
    let r = abs(f32(id.x) / uniforms.screen_width - 0.5);
    let g = abs(f32(id.y) / uniforms.screen_height - 0.5);
    output[index] = vec4<f32>(r, g, 0.0, 1.0); // Alpha set to 1.0*/
    /*let index = id.y * u32(uniforms.screen_width) + id.x;
    output[index] = vec4<f32>(1.0, 0.0, 0.0, 1.0);*/

    //output[id.y * u32(uniforms.screen_width) + id.x] =  vec3<f32>(f32(id.x)/uniforms.screen_width, f32(id.y)/uniforms.screen_height, 0.);
    /*let screen_width = uniforms.screen_width;
    let screen_height = uniforms.screen_height;

    let aspect = screen_width / screen_height;
    let fov = 0.1;
    let u = (2.0 * f32(id.x) / screen_width - 1.0) * aspect * fov;
    let v = (1.0 - 2.0 * f32(id.y) / screen_height) * fov;

    let ray = Ray(
        uniforms.camera_position.xyz,
        normalize(vec3<f32>(uniforms.camera_direction.xyz) + vec3<f32>(0.0, u, v))
    );



    //let color = trace_ray(ray);
    let color = trace_ray_bounces(ray);
    //let color = spheres[0].color;
    //let color = vec4<f32>(abs(uniforms.camera_position/10), 1.0);

    //let sphere = Sphere(1.0, vec3<f32>(10.0, 1.0, 3.), vec4<f32>(1.0, 0.0, 0.0, 1.0), vec4<f32>(1.0, 0.0, 0.0, 1.0));


    output[id.y * u32(screen_width) + id.x] = color;*/


    let aspect = uniforms.screen_width / uniforms.screen_height;
    let vertical_fov = 0.2; // vertical FOV in radians
    let half_height = tan(vertical_fov / 2.0);
    let half_width = aspect * half_height;

    let ndc_x = (2.0 * f32(id.x) / uniforms.screen_width - 1.0);
    let ndc_y = (1.0 - 2.0 * f32(id.y) / uniforms.screen_height);

    let offset_x = ndc_x * half_width;
    let offset_y = ndc_y * half_height;

    let forward = normalize(uniforms.camera_direction.xyz);

    let world_up = vec3<f32>(0.0, 0.0, 1.0);
    // Compute the right and true up vectors.
    let right = normalize(cross(forward, world_up));
    let up = cross(right, forward);


    var rand_seed = init_rand(id.x+id.y, vec4f(offset_x, offset_y, uniforms.time + ndc_x, ndc_y));

    let ray_direction = normalize(forward + right * offset_x + up * offset_y);
    var color = vec4(0.);
    for (var ray_num: u32 = 0; ray_num < rays_per_pixel; ray_num++) {

        let ray = Ray(
                uniforms.camera_position.xyz,
                ray_direction + vec3f(rand(&rand_seed), rand(&rand_seed), rand(&rand_seed))/10000
            );

        color += trace_ray_bounces(ray, &rand_seed);
    }

    let weight = 1.0/ 30;
    if uniforms.do_merging == 0. {
        //output[id.y * u32(uniforms.screen_width) + id.x] = color/f32(rays_per_pixel);
        output[id.y * (uniforms.padded_bytes_per_row / 16u) + id.x] = color/f32(rays_per_pixel);
        //output[id.y * u32(uniforms.screen_width) + id.x] = color * 0;
    } else {
        //output[id.y * u32(uniforms.screen_width) + id.x] = output[id.y * u32(uniforms.screen_width) + id.x] * (1 - weight) + color * (weight/f32(rays_per_pixel));
        output[id.y * (uniforms.padded_bytes_per_row / 16u) + id.x] = output[id.y * (uniforms.padded_bytes_per_row / 16u) + id.x] * (1 - weight) + color * (weight/f32(rays_per_pixel));
        //output[id.y * u32(uniforms.screen_width) + id.x] = color/f32(rays_per_pixel);
    }

    //output[id.y * u32(uniforms.screen_width) + id.x] = vec4f(f32(id.x)/uniforms.screen_width, f32(id.y)/uniforms.screen_height, 0., 1.);

    //output[id.y * u32(uniforms.screen_width) + id.x] = select(vec4(1.0, 0., 0., 1.), vec4(.0, 1., 0., 1.), ray_hits_triangle(triangles[0], ray).x > 0.);
    //output[id.y * u32(uniforms.screen_width) + id.x] = vec4(ray_hits_triangle(triangles[0], ray).x/1000);

    //
}