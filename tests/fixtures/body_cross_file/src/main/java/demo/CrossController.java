package demo;

import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/cross")
public class CrossController {

    @PostMapping("/msg")
    public void post(@RequestBody MessageDto dto) {}
}
